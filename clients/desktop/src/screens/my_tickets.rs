//! My tickets (AUR-FA A4, ui-ontwerp §5): the tickets that are yours, as a switch in the Board
//! page (`[ Board ] [ My tickets ]`, Ctrl+Shift+I). One read, `ticket.my_tickets` (feature
//! `my_tickets`), gives every tab and counter, so a counter and its list never differ. The rows
//! are the Board's cards. A server without the feature has no switch and no key.

use crate::app::{AppView, Page};
use crate::backend::{self, BoardCall};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::ticket::{MyTicketsView, TicketView};
use aurelius_domain::{ProjectId, TicketId, UserId};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// The part of the Board page that shows (`[ Board ] [ My tickets ]`).
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BoardPage {
    #[default]
    Board,
    MyTickets,
}

impl BoardPage {
    pub const ALL: [BoardPage; 2] = [BoardPage::Board, BoardPage::MyTickets];

    pub fn label(self) -> &'static str {
        match self {
            BoardPage::Board => "Board",
            BoardPage::MyTickets => "My tickets",
        }
    }
}

/// The four tabs of My tickets, on the filter keys 1–4.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MyTab {
    Responsible,
    Created,
    Following,
    Waiting,
}

impl MyTab {
    pub const ALL: [MyTab; 4] = [MyTab::Responsible, MyTab::Created, MyTab::Following, MyTab::Waiting];

    pub fn label(self) -> &'static str {
        match self {
            MyTab::Responsible => "Responsible",
            MyTab::Created => "Created by me",
            MyTab::Following => "Following",
            MyTab::Waiting => "Waiting on me",
        }
    }

    /// What an empty tab says (ui-ontwerp §5.3).
    pub fn empty(self) -> &'static str {
        match self {
            MyTab::Responsible => "You're not responsible for any open ticket. You'd follow up and review; employees do the work.",
            MyTab::Created => "You haven't created a ticket yet.",
            MyTab::Following => "You don't follow any open ticket. You follow one by creating it, becoming responsible, being @mentioned or pressing Follow.",
            MyTab::Waiting => "Nothing waits for you.",
        }
    }
}

/// My tickets as this window knows it.
#[derive(Default)]
pub struct MyTickets {
    pub view: Option<MyTicketsView>,
    /// Why it did not load (shown with Try again).
    pub error: Option<String>,
    /// The `(project, include_done)` of a `ticket.my_tickets` on its way.
    pub asked: Option<(Option<ProjectId>, bool)>,
    /// Asked again while that one was on its way: one more read when it answers.
    pub again: bool,
    /// An event changed a ticket while the tab did not show: read it again when it does.
    pub stale: bool,
    /// The tab shown; none until the first one is chosen (Waiting on me when something waits).
    pub tab: Option<MyTab>,
    /// "Show Done": Done and archived tickets too.
    pub include_done: bool,
}

impl MyTickets {
    /// A ticket of the last answer (the keyboard and the panel look tickets up by id).
    pub fn ticket(&self, id: TicketId) -> Option<&TicketView> {
        self.view.as_ref()?.tickets.iter().find(|t| t.id == id)
    }
}

/// Whether a ticket is in a tab for the board member `me`. `needs_me`: the ◆ on its card is theirs
/// (one definition, so the Waiting counter and the Board's badge agree).
pub fn in_tab(tab: MyTab, t: &TicketView, following: &[TicketId], me: UserId, needs_me: bool) -> bool {
    match tab {
        MyTab::Responsible => t.responsible_user_id == Some(me),
        MyTab::Created => t.created_by_user == me,
        MyTab::Following => following.contains(&t.id),
        MyTab::Waiting => needs_me,
    }
}

/// The tickets of a tab, in the order of the answer (priority, then the latest update).
pub fn tab_tickets<'a>(view: &'a MyTicketsView, tab: MyTab, me: UserId, needs_me: impl Fn(&TicketView) -> bool) -> Vec<&'a TicketView> {
    view.tickets.iter().filter(|t| in_tab(tab, t, &view.following, me, needs_me(t))).collect()
}

/// The tab to open on: Waiting on me when something waits, else Responsible.
pub fn first_tab(waiting: usize) -> MyTab {
    if waiting > 0 { MyTab::Waiting } else { MyTab::Responsible }
}

/// The label of a tab with its counter, as a button reads out: "Waiting on me 2".
pub fn tab_label(tab: MyTab, count: usize) -> String {
    format!("{} {count}", tab.label())
}

/// Whether a read goes now: not while one with the same filter is on its way.
pub fn load_now(asked: Option<(Option<ProjectId>, bool)>, project: Option<ProjectId>, include_done: bool) -> bool {
    asked != Some((project, include_done))
}

impl AppView {
    /// This server has My tickets (feature `my_tickets`) and a board member to read it for.
    pub fn my_tickets_supported(&self) -> bool {
        matches!(&self.connection, backend::Connection::Online { server, .. } if server.my_tickets) && self.active_member().is_some()
    }

    /// My tickets is what the Board page shows now: the switch is on it, on a server that has it,
    /// and not in a project's window (its Board tab is one project already).
    pub fn my_tickets_shown(&self) -> bool {
        self.screen == Page::Board && self.board_page == BoardPage::MyTickets && self.my_tickets_supported()
    }

    /// Ask the server for My tickets under the project filter and Show Done.
    pub fn my_load(&mut self) {
        if !self.my_tickets_supported() {
            return;
        }
        let project = self.board_filter();
        let include_done = self.my.include_done;
        if !load_now(self.my.asked, project, include_done) {
            self.my.again = true;
            return;
        }
        self.my.asked = Some((project, include_done));
        self.my.again = false;
        let _ = self.requests.send(backend::Request::Board(BoardCall::MyTickets { project, include_done }));
    }

    /// The answer of `ticket.my_tickets`.
    pub fn apply_my_tickets(&mut self, project: Option<ProjectId>, include_done: bool, result: Result<MyTicketsView, String>, cx: &mut Context<Self>) {
        if self.my.asked.is_some_and(|a| a != (project, include_done)) {
            return; // an older filter's answer
        }
        self.my.asked = None;
        match result {
            Ok(view) => {
                self.my.view = Some(view);
                self.my.error = None;
                self.my.stale = false;
                if self.my.tab.is_none() {
                    self.my.tab = Some(first_tab(self.my_count(MyTab::Waiting)));
                }
            }
            Err(e) => self.my.error = Some(e),
        }
        if std::mem::take(&mut self.my.again) {
            self.my_load();
        }
        cx.notify();
    }

    /// The connection went: a read on its way will not answer; read again once back.
    pub fn my_connection(&mut self, online: bool) {
        if !online && self.my.asked.take().is_some() {
            self.my.again = false;
            self.my.stale = true;
        }
    }

    /// The tickets of a tab now.
    pub fn my_tab_tickets(&self, tab: MyTab) -> Vec<&TicketView> {
        match (&self.my.view, self.active_member().map(|u| u.id)) {
            (Some(view), Some(me)) => tab_tickets(view, tab, me, |t| self.ticket_needs_me(t)),
            _ => Vec::new(),
        }
    }

    /// How many tickets a tab has.
    pub fn my_count(&self, tab: MyTab) -> usize {
        self.my_tab_tickets(tab).len()
    }

    /// The tab shown.
    pub fn my_tab(&self) -> MyTab {
        self.my.tab.unwrap_or(MyTab::Responsible)
    }

    /// Show a tab; the keyboard starts at its first card.
    pub fn set_my_tab(&mut self, tab: MyTab, cx: &mut Context<Self>) {
        self.my.tab = Some(tab);
        self.board.focused = None;
        cx.notify();
    }

    /// Filter key 1–4.
    pub fn set_my_tab_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(tab) = MyTab::ALL.get(index).copied() {
            self.set_my_tab(tab, cx);
        }
    }

    /// Show a part of the Board page; My tickets reads when it shows.
    pub fn set_board_page(&mut self, page: BoardPage, cx: &mut Context<Self>) {
        if page == BoardPage::MyTickets && !self.my_tickets_supported() {
            return;
        }
        self.board_page = page;
        self.board.focused = None;
        self.board.held = None;
        if page == BoardPage::MyTickets {
            self.my_load();
        }
        cx.notify();
    }

    /// Ctrl+Shift+I: the Board page on My tickets. A server without it says so.
    pub fn go_my_tickets(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.my_tickets_supported() {
            self.notify_user("This server can't show My tickets yet. Update the server.", false, cx);
            return;
        }
        self.go_nav(crate::app::NavItem::Board, window, cx);
        self.set_board_page(BoardPage::MyTickets, cx);
    }

    /// The palette's "Go to My tickets", only on a server that has it.
    pub fn my_tickets_palette_entries(&self) -> Vec<crate::app::PaletteEntry> {
        if !self.my_tickets_supported() || self.window_project.is_some() {
            return Vec::new();
        }
        vec![crate::app::PaletteEntry { label: "Go to My tickets".into(), hint: "General".into(), keys: Some("secondary-shift-i"), target: crate::app::PaletteTarget::Action(|| Box::new(crate::keymap::GoMyTickets)) }]
    }

    /// The switch `[ Board ] [ My tickets ]` above the Board page; none in a project's window or
    /// on a server without My tickets.
    pub fn render_board_switch(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.my_tickets_supported() || self.project_board().is_some() {
            return None;
        }
        let labels = BoardPage::ALL.map(BoardPage::label);
        let active = BoardPage::ALL.iter().position(|p| *p == self.board_page).unwrap_or(0);
        Some(self.page_tab_bar("board-switch", &labels, active, |this, i, cx| this.set_board_page(BoardPage::ALL[i], cx), cx).into_any_element())
    }

    /// The Board page's body in My tickets: header, the four tabs and the cards.
    pub fn render_my_tickets(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let online = matches!(self.connection, backend::Connection::Online { .. });
        if online && self.my.error.is_none() && self.my.asked.is_none() && (self.my.view.is_none() || self.my.stale) {
            self.my.stale = false;
            self.my_load();
        }
        let serif = ui::serif(cx);
        let project = self.board_filter().and_then(|id| self.projects.iter().find(|p| p.id == id)).map(|p| p.name.clone());
        let tab = self.my_tab();
        let loading = self.my.view.is_none();
        let waiting = self.my_count(MyTab::Waiting);
        let picker = {
            let open = self.overlay == crate::app::Overlay::Menu && self.menu.as_ref().is_some_and(|m| m.field == Some("board-project"));
            ui::dropdown_trigger("board-project", 28., project.is_some(), open)
                .max_w(px(220.))
                .on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                    let items = this.board_filter_menu("board-project");
                    this.open_dropdown("board-project", items, w, cx);
                }))
                .child(div().min_w_0().truncate().child(format!("Project: {}", project.clone().unwrap_or_else(|| "All projects".into()))))
                .child(ui::caret())
                .child(ui::track_bounds(self.dropdown_bounds.clone(), "board-project"))
        };
        let done_switch = h()
            .id("my-show-done")
            .h(px(28.))
            .px(px(10.))
            .gap(px(6.))
            .rounded(px(14.))
            .border_1()
            .cursor_pointer()
            .text_size(px(12.5))
            .role(Role::Switch)
            .aria_label("Show Done")
            .border_color(c(if self.my.include_done { theme::ATTENTION_BORDER } else { theme::BORDER }))
            .when(self.my.include_done, |el| el.bg(c(theme::ATTENTION_BG)))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.my.include_done = !this.my.include_done;
                this.my_load();
                cx.notify();
            }))
            .child("Show Done");
        let header = v().flex_none().px(px(24.)).pt(px(22.)).pb(px(12.)).gap(px(10.)).child(
            h().gap(px(10.))
                .flex_wrap()
                .child(
                    v().flex_1()
                        .min_w(px(160.))
                        .child(div().font_family(serif).text_size(px(26.)).child("My tickets"))
                        .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(format!("{waiting} wait for you"))),
                )
                .child(picker)
                .child(done_switch)
                .child(
                    ui::button("board-new", "+ New ticket", ButtonKind::Primary, Some(ui::kbd("secondary-n", cx)))
                        .when(!online, |el| el.opacity(0.5).tooltip(ui::tip("Offline · reconnecting", None)))
                        .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.board_new_ticket(w, cx))),
                ),
        );
        let tabs = h().id("my-tabs").flex_none().px(px(24.)).pb(px(10.)).gap(px(8.)).flex_wrap().children(MyTab::ALL.iter().enumerate().map(|(i, t)| {
            let on = *t == tab;
            let count = self.my_count(*t);
            h().id(("my-tab", i))
                .role(Role::Tab)
                .aria_label(SharedString::from(tab_label(*t, count)))
                .h(px(28.))
                .px(px(10.))
                .gap(px(6.))
                .rounded(px(14.))
                .border_1()
                .cursor_pointer()
                .text_size(px(12.5))
                .border_color(c(if on { theme::ATTENTION_BORDER } else { theme::BORDER }))
                .when(on, |el| el.bg(c(theme::ATTENTION_BG)))
                .when(*t == MyTab::Waiting, |el| el.child(ui::diamond(7., theme::ACCENT_TEXT, on)))
                .child(tab_label(*t, count))
                .on_click({
                    let t = *t;
                    cx.listener(move |this, _: &ClickEvent, _, cx| this.set_my_tab(t, cx))
                })
        }));
        let body: AnyElement = if let (None, Some(error)) = (&self.my.view, &self.my.error) {
            v().flex_1()
                .items_center()
                .justify_center()
                .gap(px(10.))
                .p(px(40.))
                .child(div().text_size(px(15.)).child("Could not load My tickets."))
                .child(div().max_w(px(520.)).text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(error.clone()))
                .child(ui::button("my-retry", "Try again", ButtonKind::Primary, None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.my.error = None;
                    this.my_load();
                    cx.notify();
                })))
                .into_any_element()
        } else {
            let tickets: Vec<TicketView> = self.my_tab_tickets(tab).into_iter().cloned().collect();
            let open = match self.panel {
                crate::app::Panel::Ticket(id) => Some(id),
                _ => None,
            };
            let filter_project = self.board_filter();
            let cards: Vec<AnyElement> = tickets.iter().enumerate().map(|(i, t)| self.render_card(t, i, open == Some(t.id), filter_project, online, cx)).collect();
            v().id("my-list")
                .flex_1()
                .min_h_0()
                .px(px(24.))
                .pb(px(18.))
                .gap(px(8.))
                .overflow_y_scroll()
                .when(!online, |el| el.opacity(0.6))
                .role(Role::List)
                .aria_label(SharedString::from(format!("{}, {} tickets", tab.label(), tickets.len())))
                .when(tickets.is_empty() && !loading, |el| {
                    el.child(
                        v().gap(px(8.))
                            .p(px(6.))
                            .child(div().max_w(px(520.)).text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(tab.empty()))
                            .when(tab == MyTab::Created && online, |el| el.child(ui::button("my-empty-new", "New ticket", ButtonKind::Primary, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.board_new_ticket(w, cx))))),
                    )
                })
                .when(loading && self.my.error.is_none(), |el| el.child(div().p(px(6.)).text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("Loading…")))
                .children(cards.into_iter().map(|card| div().max_w(px(720.)).child(card)))
                .into_any_element()
        };
        let mut keys = KeyContext::default();
        keys.add(crate::keymap::NAV);
        keys.add(crate::keymap::BOARD);
        v().id("board-page")
            .relative()
            .size_full()
            .min_w_0()
            .track_focus(&self.focus)
            .key_context(keys)
            .children(self.render_board_switch(cx))
            .child(header)
            .child(tabs)
            .when(!online && self.my.view.is_some(), |el| el.child(div().px(px(24.)).pb(px(6.)).text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Offline · reconnecting. My tickets shows the last known state.")))
            .child(body)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{BoardPage, MyTab, first_tab, in_tab, load_now, tab_label, tab_tickets};
    use aurelius_domain::ticket::{MyTicketsView, TicketView};
    use aurelius_domain::{ProjectId, TicketId, UserId};

    fn view_of(tickets: Vec<TicketView>, following: Vec<TicketId>) -> MyTicketsView {
        MyTicketsView { tickets, following }
    }

    fn ticket(number: u32, responsible: Option<UserId>, creator: UserId) -> TicketView {
        let mut t = crate::screens::board::tests::card(number, aurelius_domain::ticket::ColumnKind::Review);
        t.created_by_user = creator;
        t.responsible_user_id = responsible;
        t
    }

    #[test]
    fn the_four_tabs_and_their_words_are_the_designs() {
        assert_eq!(MyTab::ALL.map(MyTab::label), ["Responsible", "Created by me", "Following", "Waiting on me"]);
        assert_eq!(BoardPage::ALL.map(BoardPage::label), ["Board", "My tickets"]);
        assert_eq!(MyTab::Waiting.empty(), "Nothing waits for you.");
        assert_eq!(MyTab::Created.empty(), "You haven't created a ticket yet.");
        assert!(MyTab::Responsible.empty().starts_with("You're not responsible for any open ticket."));
        assert!(MyTab::Following.empty().contains("pressing Follow."));
        assert_eq!(tab_label(MyTab::Waiting, 2), "Waiting on me 2");
    }

    #[test]
    fn a_ticket_is_in_the_tab_it_belongs_to_and_the_counter_is_the_list() {
        let me = UserId::new();
        let other = UserId::new();
        let mine = ticket(1, Some(me), other);
        let created = ticket(2, None, me);
        let followed = ticket(3, Some(other), other);
        let waits = ticket(4, Some(other), other);
        let view = view_of(vec![mine.clone(), created.clone(), followed.clone(), waits.clone()], vec![followed.id]);
        let waiting = |t: &TicketView| t.id == waits.id;
        let numbers = |tab| tab_tickets(&view, tab, me, waiting).iter().map(|t| t.number).collect::<Vec<_>>();
        assert_eq!(numbers(MyTab::Responsible), [1]);
        assert_eq!(numbers(MyTab::Created), [2]);
        assert_eq!(numbers(MyTab::Following), [3]);
        assert_eq!(numbers(MyTab::Waiting), [4]);
        assert!(in_tab(MyTab::Waiting, &waits, &[], me, true));
        assert!(!in_tab(MyTab::Waiting, &waits, &[], me, false), "needs_me is the only rule of Waiting on me");
    }

    #[test]
    fn it_opens_on_waiting_only_when_something_waits() {
        assert_eq!(first_tab(2), MyTab::Waiting);
        assert_eq!(first_tab(0), MyTab::Responsible);
    }

    #[test]
    fn a_read_with_the_same_filter_is_not_asked_twice() {
        let p = Some(ProjectId::new());
        assert!(load_now(None, p, false));
        assert!(!load_now(Some((p, false)), p, false));
        assert!(load_now(Some((p, false)), p, true), "Show Done is another read");
        assert!(load_now(Some((p, false)), None, false));
    }

    #[test]
    fn ctrl_shift_i_is_my_tickets_and_nothing_else_has_it() {
        let own: Vec<_> = crate::keymap::SHORTCUTS.iter().filter(|s| s.keys.contains(&"secondary-shift-i")).collect();
        assert_eq!(own.len(), 1, "one binding");
        assert!(own[0].label.starts_with("Go to My tickets"));
        assert!(!own[0].in_palette, "the palette lists it only for a server that has My tickets");
    }
}
