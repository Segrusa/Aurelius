//! The live preview pane beside "At work now", and the live parts of the Tasks tab's detail: what an employee or
//! task does right now, from the streams and messages the app already gets (no extra protocol).

use crate::app::{AppView, ManagerRef, Page};
use crate::backend;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::conversation::MessageRole;
use aurelius_domain::task::TaskStatus;
use aurelius_domain::{ConversationId, EmployeeId};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Below this window width the preview pane is left out (the list keeps the room).
pub const PREVIEW_MIN_WINDOW: f32 = 1200.;

/// The end of a streaming answer: its last `max` characters, cut at a word.
pub fn text_tail(text: &str, max: usize) -> String {
    let text = text.trim();
    let count = text.chars().count();
    if count <= max {
        return text.to_string();
    }
    let rest: String = text.chars().skip(count - max).collect();
    let rest = rest.split_once(char::is_whitespace).map_or(rest.as_str(), |(_, r)| r);
    format!("…{}", rest.trim_start())
}

impl AppView {
    /// The manager the preview pane shows: Home's selected "At work now" row, or the
    /// selected card of the Managers tab. `None` after Esc until the selection moves.
    pub fn preview_manager(&self) -> Option<EmployeeId> {
        if self.preview_closed {
            return None;
        }
        match self.screen {
            Page::Dashboard => self.at_work_selected.and_then(|n| self.working_now().get(n).map(|w| w.manager)),
            _ => None,
        }
    }

    /// Whether a conversation's stream shows outside the open conversation: in the preview pane.
    pub fn stream_shown_beside(&self, conversation: ConversationId) -> bool {
        self.preview_manager().and_then(|id| self.managers.iter().find(|m| m.id == id))
            .is_some_and(|m| m.agents.iter().any(|a| a.conversation_id == Some(conversation)))
    }

    pub fn preview_shown(&self) -> bool {
        self.preview_manager().is_some()
    }

    /// Ask once for the messages of the conversation in the preview (again after a run starts or ends).
    pub fn ensure_preview_messages(&mut self, conversation: ConversationId) {
        if self.preview_requested == Some(conversation) {
            return;
        }
        self.preview_requested = Some(conversation);
        if self.preview_messages.as_ref().is_some_and(|m| m.conversation_id != conversation) {
            self.preview_messages = None;
        }
        let _ = self.requests.send(backend::Request::LoadPreviewMessages(conversation));
    }

    /// "NOW": how long the answer runs, its latest tool use and the end of its text.
    pub fn live_section(&self, conversation: ConversationId, cx: &mut Context<Self>) -> Option<Div> {
        let s = self.streams.get(&conversation)?;
        let tail = text_tail(&s.text, 360);
        let idle = s.activity.is_empty() && tail.is_empty();
        Some(
            v().gap(px(8.)).flex_none()
                .child(
                    h().gap(px(8.)).child(ui::eyebrow("NOW")).child(div().flex_1())
                        .when_some(s.progress.as_ref(), |el, p| el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(ui::fmt_run(p)))),
                )
                .when_some(s.activity.last(), |el, a| el.child(self.activity_card(s.message_id, a, cx)))
                .when(!tail.is_empty(), |el| el.child(div().text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::TEXT_2)).child(tail)))
                .when(idle, |el| el.child(ui::thinking("preview-thinking"))),
        )
    }

    /// "RECENT": the last few messages of the conversation, oldest first.
    pub fn recent_messages(&self, conversation: ConversationId) -> Option<Div> {
        let snapshot = self.preview_messages.as_ref().filter(|m| m.conversation_id == conversation)?;
        let mut last: Vec<_> = snapshot.messages.iter().rev().filter(|m| !m.text.trim().is_empty()).take(4).collect();
        if last.is_empty() {
            return None;
        }
        last.reverse();
        Some(v().gap(px(8.)).flex_none().child(ui::eyebrow("RECENT")).children(last.into_iter().map(|m| {
            let who = match (m.role, &m.author) {
                (MessageRole::User, _) => "You".to_string(),
                (_, Some(author)) => author.clone(),
                _ => "Manager".to_string(),
            };
            v().gap(px(3.)).px(px(12.)).py(px(9.)).rounded(px(7.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                .child(h().gap(px(6.)).text_size(px(11.5)).text_color(c(theme::MUTED)).child(who).child(format!("· {}", crate::screens::projects::clock(m.created_at_ms))))
                .child(div().text_size(px(12.5)).line_height(px(18.)).line_clamp(4).child(m.text.trim().to_string()))
        })))
    }

    /// The pane beside the list for one manager: status, what it does now, its open tasks, recent messages.
    pub fn render_manager_preview(&mut self, id: EmployeeId, cx: &mut Context<Self>) -> AnyElement {
        let Some(m) = self.managers.iter().find(|m| m.id == id).cloned() else { return div().into_any_element() };
        let serif = ui::serif(cx);
        // The conversation it works in now, else its latest task's.
        let conversation = m.agents.iter().filter_map(|a| a.conversation_id).find(|c| self.streams.contains_key(c))
            .or_else(|| m.agents.iter().find_map(|a| a.conversation_id))
            .or_else(|| self.tasks.iter().filter(|t| t.employee_id == Some(id) && t.conversation_id.is_some()).max_by_key(|t| t.updated_at_ms).and_then(|t| t.conversation_id));
        if let Some(conversation) = conversation {
            self.ensure_preview_messages(conversation);
        }
        let mut open: Vec<_> = self.tasks.iter().filter(|t| t.employee_id == Some(id) && ui::is_active(t)).cloned().collect();
        open.sort_by_key(|t| (t.status != TaskStatus::Running, t.created_at_ms));
        let status = if m.status == aurelius_domain::employee::LifecycleStatus::Paused {
            h().gap(px(6.)).text_color(c(theme::MUTED)).child(ui::ring(6., theme::MUTED)).child("Paused")
        } else if m.working {
            h().gap(px(6.)).text_color(c(theme::ACCENT_TEXT)).child(ui::pulse_dot("preview-pulse", 6., theme::ACCENT)).child("Working")
        } else {
            h().gap(px(6.)).text_color(c(theme::MUTED)).child(ui::ring(6., theme::MUTED)).child("Idle")
        };
        let doing = crate::app::work_of(&m.agents);
        let manager = ManagerRef::Real(id);
        v().size_full().border_l_1().border_color(c(theme::BORDER)).bg(c(theme::PANEL))
            .child(
                div().id("manager-preview").size_full().overflow_y_scrollbar().px(px(24.)).py(px(22.)).flex().flex_col().gap(px(18.))
                    .child(h().justify_between().text_size(px(11.5)).child(ui::eyebrow("EMPLOYEE")).child(status))
                    .child(
                        h().gap(px(12.)).flex_none()
                            .child(ui::monogram(&m.name, 36., cx))
                            .child(v().flex_1().min_w_0()
                                .child(div().truncate().font_family(serif).text_size(px(22.)).child(m.name.clone()))
                                .child(div().truncate().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(format!("{} · {}", m.role, m.agent_mode.label())))),
                    )
                    .child(
                        h().gap(px(8.)).flex_none().child(
                            ui::button("preview-open", "Open", ButtonKind::Outline, Some(ui::kbd("enter", cx))).h(px(28.))
                                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.open_manager(manager, w, cx))),
                        ),
                    )
                    .when_some(doing, |el, (title, since)| {
                        el.child(div().text_size(px(13.)).text_color(c(theme::TEXT_2)).child(if since > 0 {
                            format!("{title} · {}", ui::fmt_elapsed(ui::now_ms() - since))
                        } else {
                            title
                        }))
                    })
                    .children(conversation.and_then(|c| self.live_section(c, cx)))
                    .child(
                        v().gap(px(6.)).flex_none().child(ui::eyebrow(format!("OPEN TASKS · {}", open.len())))
                            .when(open.is_empty(), |el| el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("No open tasks.")))
                            .children(open.iter().enumerate().map(|(i, t)| {
                                let busy = matches!(t.status, TaskStatus::Running | TaskStatus::Review);
                                let target = t.conversation_id.map(|conv| (t.project_id, conv));
                                h().id(("preview-task", i)).gap(px(8.)).px(px(10.)).py(px(8.)).rounded(px(7.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                                    .when(target.is_some(), |el| el.cursor_pointer().hover(|s| s.border_color(c(theme::BORDER_STRONG))))
                                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                                        if let Some((project, conv)) = target {
                                            this.open_task_thread(project, conv, w, cx);
                                        }
                                    }))
                                    .child(if busy { ui::pulse_dot(("preview-task-busy", i), 6., theme::ACCENT) } else { ui::ring(6., theme::MUTED).into_any_element() })
                                    .child(div().flex_1().min_w_0().truncate().text_size(px(12.5)).child(t.title.clone()))
                                    .when(t.pending_approvals > 0, |el| el.child(ui::diamond(8., theme::ACCENT_TEXT, false)))
                                    .child(div().flex_none().text_size(px(11.5)).text_color(c(theme::MUTED)).child(self.task_mark(t).label()))
                            })),
                    )
                    .children(conversation.and_then(|c| self.recent_messages(c))),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::text_tail;

    #[test]
    fn the_tail_keeps_short_text_and_cuts_long_text_at_a_word() {
        assert_eq!(text_tail("  short  ", 20), "short");
        assert_eq!(text_tail("one two three four", 9), "…four");
        assert_eq!(text_tail("éééé ü", 3), "…ü");
    }
}
