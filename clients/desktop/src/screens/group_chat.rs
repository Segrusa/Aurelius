//! The group chat ("Team chat"): one conversation of the user with every employee,
//! right under Dashboard. `@Name` asks that employee; without a name the top of the
//! org chart answers. Each answer shows under its employee's name and streams in live.

use super::chat::composer;
use crate::app::AppView;
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, h, v};
use aurelius_domain::employee::LifecycleStatus;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

impl AppView {
    pub fn render_group_chat(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let team = self.managers.iter().filter(|m| m.status == LifecycleStatus::Active).count();
        // Who is writing an answer here right now.
        let answering: Vec<String> = self.group_live.iter().filter_map(|(_, run, _)| self.run_author(*run)).collect();
        let header = h()
            .h(px(68.))
            .flex_none()
            .px(px(28.))
            .gap(px(14.))
            .border_b_1()
            .border_color(c(theme::BORDER))
            .child(
                v().flex_1().min_w_0()
                    .child(div().text_size(px(15.)).font_weight(FontWeight::MEDIUM).child(crate::app::NavItem::Chat.label()))
                    .child(div().truncate().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(match team {
                        1 => "You and 1 employee".to_string(),
                        n => format!("You and {n} employees"),
                    })),
            )
            .when(!answering.is_empty(), |el| {
                el.child(h().gap(px(7.)).text_size(px(12.5)).child(ui::pulse_dot("group-answering", 6., theme::ACCENT)).child(format!("{} answering", answering.join(", "))))
            });
        v().flex_1()
            .min_w_0()
            .h_full()
            // Files dropped anywhere on the conversation go with the next message.
            .drag_over::<ExternalPaths>(|style, _, _, _| style.bg(c(theme::SURFACE)))
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| this.drop_files(paths.paths(), cx)))
            .child(header)
            .child(self.conversation_pane(|this, _, cx| this.group_scroll(cx), cx))
            .child(composer(self, "", false, cx))
            .into_any_element()
    }

    /// The group chat's scroll area, in the conversation pane.
    fn group_scroll(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let empty = self.messages.as_ref().is_some_and(|s| s.messages.is_empty()) && self.group_live.is_empty();
        let body = v().w_full().max_w(px(700.)).gap(px(16.))
            .when(empty, |el| {
                let first = self.group_answerer().map_or_else(|| "the top of the org chart".to_string(), str::to_owned);
                el.child(
                    v().gap(px(6.)).text_size(px(13.5)).text_color(c(theme::TEXT_2))
                        .child(div().text_size(px(15.)).text_color(c(theme::TEXT)).child("Talk with your whole team here."))
                        .child(format!("Write @Name to ask an employee; without a name, {first} answers and brings in whoever is needed.")),
                )
            })
            .child(self.render_messages("Aurelius".into(), cx));
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
