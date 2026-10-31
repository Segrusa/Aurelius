//! The group chat ("Team chat"): one conversation of the user with every employee,
//! right under Dashboard. `@Name` asks that employee; without a name the top of the
//! org chart answers. Each answer shows under its employee's name and streams in live.
//!
//! Channels (AUR-MU-02, fase 2 §3): #general plus one per project, listed on the
//! left; an archived project's channel is read-only. A server without channels
//! (no `channel.list`) keeps the one general chat and no list shows.

use super::chat::composer;
use crate::app::AppView;
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, h, v, ButtonKind};
use crate::screens::discussion as d;
use aurelius_domain::ConversationId;
use aurelius_domain::employee::LifecycleStatus;
use aurelius_domain::views::ChannelView;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl AppView {
    /// `channel.list` answered (none: the server has no channels).
    pub fn channels_loaded(&mut self, channels: Option<Vec<ChannelView>>) {
        // A channel that is gone (its project was deleted): back to general.
        if let (Some(list), Some(open)) = (&channels, self.channel)
            && !list.iter().any(|c| c.conversation_id == open)
        {
            self.channel = None;
            self.group_conversation = None;
            if self.screen == crate::app::Page::GroupChat {
                self.open(None);
                let _ = self.requests.send(crate::backend::Request::OpenGroupChat(None));
            }
        }
        self.channels = channels.filter(|list| !list.is_empty());
    }

    /// The channel the Team chat shows, when the server has channels.
    pub fn current_channel(&self) -> Option<&ChannelView> {
        let list = self.channels.as_ref()?;
        match self.channel {
            Some(id) => list.iter().find(|c| c.conversation_id == id),
            None => list.iter().find(|c| c.project_id.is_none()),
        }
    }

    /// The project of the open channel (none in #general).
    pub fn channel_project(&self) -> Option<aurelius_domain::ProjectId> {
        self.current_channel().and_then(|c| c.project_id)
    }

    /// The composer's placeholder in the Team chat (fase 2 §3).
    pub fn channel_placeholder(&self) -> String {
        match self.current_channel() {
            None => crate::app::GROUP_PLACEHOLDER.into(),
            c => d::channel_hint(c),
        }
    }

    /// Choose the channel the Team chat shows; it loads when the Team chat opens it.
    /// Returns it, or none when it is not in the list or already open.
    fn choose_channel(&mut self, conversation: ConversationId) -> Option<ChannelView> {
        let target = self.channels.as_ref().and_then(|l| l.iter().find(|c| c.conversation_id == conversation)).cloned()?;
        let channel = target.project_id.map(|_| conversation);
        if channel == self.channel && self.group_conversation == Some(conversation) {
            return None;
        }
        self.channel = channel;
        self.channel_divider = (target.unread > 0).then_some((conversation, target.unread as usize));
        self.channel_new_at = None;
        self.group_conversation = None;
        self.group_error = None;
        Some(target)
    }

    /// Ctrl+K "Go to #…": the Team chat on that channel, from any page. The channel is
    /// chosen before the page opens, so no other channel is read (and marked read) on the way.
    pub fn go_to_channel(&mut self, conversation: ConversationId, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen == crate::app::Page::GroupChat {
            return self.open_channel(conversation, window, cx);
        }
        self.choose_channel(conversation);
        self.go_nav(crate::app::NavItem::Chat, window, cx);
    }

    /// Show a channel: its messages, with "New" before the ones not read yet.
    pub fn open_channel(&mut self, conversation: ConversationId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(target) = self.choose_channel(conversation) else { return };
        self.open(None);
        let placeholder = self.channel_placeholder();
        self.pending_placeholder_set(placeholder);
        let _ = self.requests.send(crate::backend::Request::OpenGroupChat(self.channel));
        if !target.archived {
            self.composer.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    /// Alt+↑ / Alt+↓: the channel above or below in the list.
    pub fn step_channel(&mut self, down: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(list) = self.channels.clone() else { return };
        let current = self.current_channel().map(|c| c.conversation_id);
        if let Some(next) = d::step_channel(&list, current, down) {
            self.open_channel(next, window, cx);
        }
    }

    /// The Team chat's sidebar badge (B8): mentions, and a dot while anything is unread.
    pub fn team_chat_badge(&self) -> (u32, bool) {
        self.channels.as_deref().map_or((0, false), d::badge)
    }

    /// The channel list on the left of the Team chat (fase 2 §3).
    fn render_channel_list(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let list = self.channels.as_ref()?;
        let (general, projects, archived) = d::ordered(list);
        let current = self.current_channel().map(|c| c.conversation_id);
        let row = |ch: &ChannelView, i: usize, cx: &mut Context<Self>| {
            let on = current == Some(ch.conversation_id);
            let id = ch.conversation_id;
            let bold = ch.unread > 0 && !on;
            h().id(SharedString::from(format!("channel-{i}"))).role(Role::Tab).aria_label(if on { format!("{}, selected", d::channel_label(ch)) } else { d::channel_label(ch) })
                .gap(px(6.)).px(px(10.)).py(px(4.)).rounded(px(6.)).cursor_pointer()
                .text_size(px(12.5))
                .text_color(c(if on || bold { theme::TEXT } else { theme::TEXT_2 }))
                .when(on, |el| el.bg(c(theme::SURFACE_2)))
                .when(bold, |el| el.font_weight(FontWeight::SEMIBOLD))
                .hover(|s| s.bg(c(theme::SURFACE_2)))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_channel(id, w, cx)))
                .child(div().flex_1().min_w_0().truncate().child(d::channel_title(ch)))
                // A mention of a board member: an "@" badge, never colour alone (P-24).
                .when(ch.mentions > 0, |el| el.child(div().px(px(5.)).rounded(px(8.)).bg(c(theme::ACCENT)).text_color(c(theme::BG)).text_size(px(11.)).child(format!("@{}", ch.mentions))))
                .when(ch.unread > 0 && ch.mentions == 0 && !on, |el| el.child(div().px(px(5.)).rounded(px(8.)).bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT_2)).text_size(px(11.)).child(ch.unread.to_string())))
                .when(ch.archived, |el| el.child(div().text_size(px(11.)).text_color(c(theme::TEXT_2)).child("read-only")))
        };
        let mut i = 0;
        let mut next = || {
            i += 1;
            i
        };
        let general_row = general.map(|g| row(g, next(), cx).into_any_element());
        let project_rows: Vec<AnyElement> = projects.iter().map(|ch| row(ch, next(), cx).into_any_element()).collect();
        let archived_rows: Vec<AnyElement> = if self.channels_archived_open { archived.iter().map(|ch| row(ch, next(), cx).into_any_element()).collect() } else { Vec::new() };
        let open = self.channels_archived_open;
        Some(
            v().id("channel-list").role(Role::TabList).aria_label("Channels").w(px(180.)).flex_none().h_full().overflow_y_scroll().gap(px(2.)).px(px(8.)).py(px(14.)).border_r_1().border_color(c(theme::BORDER))
                .child(div().px(px(10.)).pb(px(4.)).child(ui::eyebrow("CHANNELS")))
                .children(general_row)
                .when(!project_rows.is_empty(), |el| el.child(div().px(px(10.)).pt(px(12.)).pb(px(4.)).child(ui::eyebrow("PROJECTS"))))
                .children(project_rows)
                .when(!archived.is_empty(), |el| {
                    el.child(
                        div().id("channels-archived").role(Role::Button).aria_label(if open { "Archived, expanded" } else { "Archived, collapsed" }).px(px(10.)).pt(px(12.)).pb(px(4.)).cursor_pointer().text_size(px(11.5)).text_color(c(theme::TEXT_2))
                            .child(format!("{} Archived ({})", if open { "▾" } else { "▸" }, archived.len()))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                                this.channels_archived_open = !this.channels_archived_open;
                                cx.notify();
                            })),
                    )
                })
                .children(archived_rows)
                .into_any_element(),
        )
    }

    pub fn render_group_chat(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let team = self.managers.iter().filter(|m| m.status == LifecycleStatus::Active).count();
        // Who is writing an answer here right now.
        let answering: Vec<String> = self.group_live.iter().filter_map(|l| l.author.clone().or_else(|| self.run_author(l.run))).collect();
        let header = h()
            .h(px(68.))
            .flex_none()
            .px(px(28.))
            .gap(px(14.))
            .border_b_1()
            .border_color(c(theme::BORDER))
            .child(
                v().flex_1().min_w_0()
                    .child(div().text_size(px(15.)).font_weight(FontWeight::MEDIUM).child(self.current_channel().map_or_else(|| crate::app::NavItem::Chat.label().to_string(), d::channel_title)))
                    .child(div().truncate().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(match (self.current_channel(), team) {
                        (Some(ch), _) if ch.archived => "Archived project · read-only".to_string(),
                        (Some(ch), 1) if ch.project_id.is_some() => "Project channel · you and 1 employee".to_string(),
                        (Some(ch), n) if ch.project_id.is_some() => format!("Project channel · you and {n} employees"),
                        (_, 1) => "You and 1 employee".to_string(),
                        (_, n) => format!("You and {n} employees"),
                    })),
            )
            .when(!answering.is_empty(), |el| {
                el.child(h().gap(px(7.)).text_size(px(12.5)).child(ui::pulse_dot("group-answering", 6., theme::ACCENT)).child(format!("{} answering", answering.join(", "))))
            });
        let read_only = self.current_channel().is_some_and(|c| c.archived);
        let list = self.render_channel_list(cx);
        let chat = v().flex_1()
            .min_w_0()
            .h_full()
            // Files dropped anywhere on the conversation go with the next message.
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(c(theme::SURFACE)))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.drop_files(paths.paths(), cx)))
            .child(header)
            .child(self.conversation_pane(|this, _, cx| this.group_scroll(cx), cx))
            .map(|el| if read_only {
                el.child(div().flex_none().px(px(28.)).py(px(14.)).border_t_1().border_color(c(theme::BORDER)).text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(d::channel_hint(self.current_channel())))
            } else {
                el.child(composer(self, "", false, cx))
            });
        match list {
            Some(list) => h().flex_1().min_w_0().h_full().items_start().child(list).child(chat).into_any_element(),
            None => chat.into_any_element(),
        }
    }

    /// What employees' runs ask from their main chat (where group chat answers are written)
    /// with no answer streaming here to show it under: decided right here, under the asker's name.
    pub fn group_approvals(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let asks: Vec<_> = self.pending_approvals()
            .filter(|a| a.conversation_id.is_some_and(|id| self.is_main_chat(id)) && !self.group_live.iter().any(|l| l.run == a.run_id))
            .cloned()
            .collect();
        asks.into_iter().map(|a| {
            let (label, body): (&str, AnyElement) = if a.tool == crate::app::QUESTION_TOOL {
                ("QUESTION · WAITING FOR YOU", self.question_form(&a, None, cx).into_any_element())
            } else if a.tool == crate::app::PLAN_TOOL {
                ("PLAN · WAITING FOR YOU", div().text_size(px(13.5)).text_color(c(theme::TEXT_2)).child("A plan to decide on.").into_any_element())
            } else {
                let summary = crate::screens::approval::approval_summary(&a, &self.approval_folders(&a));
                ("WAITING FOR YOU", v().gap(px(8.))
                    .child(div().text_size(px(13.5)).text_color(c(theme::TEXT)).child(summary.title))
                    .child(self.approval_detail(&a, "group", cx))
                    .into_any_element())
            };
            let id = a.id;
            v().gap(px(6.)).px(px(14.)).py(px(11.)).rounded(px(8.)).border_1()
                .border_color(c(theme::ATTENTION_BORDER)).bg(c(theme::ATTENTION_BG))
                .child(h().gap(px(8.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child(ui::eyebrow(label)).child(div().flex_1().min_w_0().text_color(c(theme::TEXT_2)).child(a.requester.clone())))
                .child(body)
                .child(self.approval_actions(a, "group", cx))
                .id(SharedString::from(format!("group-ask-{id}")))
                .into_any_element()
        }).collect()
    }

    /// The group chat's scroll area, in the conversation pane.
    fn group_scroll(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let empty = self.messages.as_ref().is_some_and(|s| s.messages.is_empty()) && self.group_live.is_empty();
        // It did not load: say why, instead of "Loading…" for good.
        let failed = self.group_conversation.is_none().then(|| self.group_error.clone()).flatten();
        let body = v().w_full().max_w(px(700.)).gap(px(16.))
            .when_some(failed.clone(), |el, why| {
                el.child(
                    v().gap(px(10.)).text_size(px(13.5)).text_color(c(theme::TEXT_2))
                        .child(div().text_size(px(15.)).text_color(c(theme::TEXT)).child("The Team chat did not load."))
                        .child(why)
                        .child(h().child(ui::button("group-retry", "Try again", ButtonKind::Outline, None).h(px(28.))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.retry_group_chat(cx))))),
                )
            })
            .when(empty, |el| {
                let first = self.group_answerer().map_or_else(|| "the top of the org chart".to_string(), str::to_owned);
                el.child(
                    v().gap(px(6.)).text_size(px(13.5)).text_color(c(theme::TEXT_2))
                        .child(div().text_size(px(15.)).text_color(c(theme::TEXT)).child("Talk with your whole team here."))
                        .child(format!("Write @Name to ask an employee; without a name, {first} answers and brings in whoever is needed.")),
                )
            })
            .when(failed.is_none(), |el| el.child(self.render_messages("Aurelius".into(), cx)));
        ui::scroll_frame(&self.scroll,
            div()
                .id("group-scroll")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .track_focus(&self.focus)
                .key_context(NAV)
                .flex()
                .flex_col()
                .items_center()
                .px(px(36.))
                .pt(px(26.))
                .pb(px(16.))
                .child(ui::rise(div().flex_none().w_full().flex().justify_center().child(body), ("page", self.page_key()), 0)),
        ).flex_1().into_any_element()
    }
}
