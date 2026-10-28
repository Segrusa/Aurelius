//! A plan Claude handed in (ExitPlanMode), reviewed in a drawer on the right:
//! the plan as Markdown (scrolls), a feedback field, and four buttons.
//! Goedkeuren + Auto: the agent goes on in auto mode. Goedkeuren: the agent goes on
//! in accept-edits mode and asks for the rest. Pas aan: feedback is
//! required and the agent revises the plan. Reject: the agent stops.

use crate::app::{AppView, PlanStatus};
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::{ApprovalId, approval::PlanDecision};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::component::scroll::ScrollableElement as _;

/// The line under the drawer's header once the plan is no longer open, with its colour.
fn status_line(status: PlanStatus) -> Option<(&'static str, u32)> {
    match status {
        PlanStatus::Open => None,
        PlanStatus::Sending(_) => Some(("Sending your decision…", theme::TEXT_2)),
        PlanStatus::Decided(PlanDecision::Approve) => Some(("Approved. The agent goes on in auto mode.", theme::ACCENT_TEXT)),
        PlanStatus::Decided(PlanDecision::ApproveEdits) => Some(("Approved. Edits go through; the agent asks for the rest.", theme::ACCENT_TEXT)),
        PlanStatus::Decided(PlanDecision::Revise) => Some(("Changes requested. The agent revises the plan.", 0xd99a3a)),
        PlanStatus::Decided(PlanDecision::Reject) => Some(("Rejected. The agent stops.", 0xb07a70)),
        PlanStatus::HandledElsewhere => Some(("Already handled on another device.", theme::TEXT_2)),
        PlanStatus::Expired => Some(("Expired. Nothing was approved; expiry never counts as permission.", 0xd99a3a)),
    }
}

impl AppView {
    /// The plan in a panel that slides in from the right, with the three decisions.
    pub(crate) fn render_plan_approval_panel(&mut self, id: ApprovalId, cx: &mut Context<Self>) -> AnyElement {
        let approval = self.plan_snapshot.clone().filter(|a| a.id == id);
        let markdown = approval.as_ref().map(crate::app::plan_text_of).filter(|t| !t.trim().is_empty()).unwrap_or_else(|| "This plan has no text.".to_string());
        let requester = approval.as_ref().map(|a| a.requester.clone()).unwrap_or_default();
        let status = self.plan_status(id);
        let open = status == PlanStatus::Open;
        let has_feedback = !self.plan_input.read(cx).value().trim().is_empty() || !self.plan_comments.is_empty();
        let version = {
            use std::hash::{Hash, Hasher};
            let mut h = std::collections::hash_map::DefaultHasher::new();
            markdown.hash(&mut h);
            h.finish() as usize
        };
        // A button that cannot be pressed is dimmed and has no click handler.
        let dim = |el: Stateful<Div>, enabled: bool| if enabled { el } else { el.opacity(0.4).cursor_default() };

        let approve = dim(ui::button("plan-approval-approve", "Goedkeuren + Auto", ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx))), open)
            .when(open, |el| el.on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.decide_plan(PlanDecision::Approve, w, cx))));
        let approve_edits = dim(ui::button("plan-approval-approve-edits", "Goedkeuren", ButtonKind::Outline, None), open)
            .when(open, |el| el.on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.decide_plan(PlanDecision::ApproveEdits, w, cx))));
        let changes = dim(ui::button("plan-approval-changes", "Pas aan", ButtonKind::Outline, None), open && has_feedback)
            .when(open && has_feedback, |el| el.on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.decide_plan(PlanDecision::Revise, w, cx))));
        let reject = dim(ui::button("plan-approval-reject", "Reject", ButtonKind::Outline, None), open)
            .when(open, |el| el.on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.decide_plan(PlanDecision::Reject, w, cx))));

        let panel = v()
            .w(px(660.))
            .max_w(relative(0.92))
            .h_full()
            .bg(c(theme::PANEL))
            .border_l_1()
            .border_color(c(theme::BORDER_STRONG))
            .shadow_lg()
            .track_focus(&self.overlay_focus)
            .key_context(PLAN_REVIEW)
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(
                h().px(px(28.)).py(px(16.)).gap(px(10.)).border_b_1().border_color(c(theme::BORDER))
                    .child(ui::diamond(9., theme::ACCENT_TEXT, open))
                    .child(div().flex_1().child(ui::eyebrow(if open { "PLAN · WAITING FOR YOU" } else { "PLAN" })))
                    .when(!requester.is_empty(), |el| el.child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(requester.clone())))
                    .child(ui::close_button(cx)),
            )
            .when_some(status_line(status), |el, (text, color)| {
                el.child(
                    div().id("plan-approval-status").px(px(28.)).py(px(10.)).border_b_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                        .text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(c(color)).child(text),
                )
            })
            .child(
                div().id("plan-approval-body").flex_1().on_mouse_up(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, w, cx| {
                    let selected = gpui_kit::base::TextSelection::selected_text(w, cx).trim().to_string();
                    this.plan_selection = (!selected.is_empty()).then_some(selected);
                    cx.notify();
                })).min_h_0().overflow_y_scrollbar().px(px(32.)).py(px(26.))
                    .text_color(c(theme::TEXT))
                    .text_size(px(14.))
                    .child(gpui_kit::component::text::TextView::markdown(("plan-approval-md", version), markdown).selectable(true).plugin(crate::screens::mermaid::Mermaid).on_link_click(self.markdown_link_handler(false, cx))),
            )
            .child(
                v().px(px(24.)).py(px(14.)).gap(px(10.)).border_t_1().border_color(c(theme::BORDER)).bg(c(theme::PANEL)).shadow_lg()
                    .when(open, |el| el.child(self.render_plan_comments(cx)))
                    .when(open || matches!(status, PlanStatus::Sending(_)), |el| el.child(ui::input(&self.plan_input)))
                    .child(
                        h().gap(px(10.)).justify_end().flex_wrap()
                            .child(changes)
                            .child(reject)
                            .child(approve_edits)
                            .child(approve),
                    ),
            );
        let key = self.overlay_key();
        deferred(ui::fade_in(
            div()
                .id("plan-approval-scrim")
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(Hsla { a: 0.45, ..c(0x000000) })
                .flex()
                .justify_end()
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|_, _, w, cx| w.dispatch_action(Box::new(Back), cx)))
                .child(panel.with_animation(("plan-approval-slide", key), Animation::new(std::time::Duration::from_millis(280)), |el, t| {
                    let e = 1.0 - (1.0 - t).powi(4);
                    el.opacity(e).relative().left(px((1.0 - e) * 56.0))
                })),
            ("plan-approval-scrim", key),
            140,
        ))
        .with_priority(1)
        .into_any_element()
    }
}

/// A fragment cut to a line that fits the comment list.
fn short_quote(quote: &str) -> String {
    let flat = quote.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() > 140 { format!("{}…", flat.chars().take(140).collect::<String>()) } else { flat }
}

impl AppView {
    /// Under the plan: the 'Comment' offer for the selected text, the comment field while
    /// writing one, and the list of comments so far (each removable).
    fn render_plan_comments(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let offer = match (&self.plan_comment_draft, &self.plan_selection) {
            (Some(quote), _) => Some(
                v().gap(px(8.))
                    .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("Comment on: “{}”", short_quote(quote))))
                    .child(ui::input(&self.plan_comment_input))
                    .child(
                        h().gap(px(8.)).justify_end()
                            .child(ui::button("plan-comment-cancel", "Cancel", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                                this.plan_comment_draft = None;
                                this.plan_comment_input.update(cx, |input, cx| input.set_value("", w, cx));
                                cx.notify();
                            })))
                            .child(ui::button("plan-comment-add", "Add comment", ButtonKind::Primary, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.add_plan_comment(w, cx)))),
                    )
                    .into_any_element(),
            ),
            (None, Some(selected)) => {
                let selected = selected.clone();
                Some(
                    h().gap(px(8.)).items_center()
                        .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("Selected: “{}”", short_quote(&selected))))
                        .child(ui::button("plan-comment-start", "Comment", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                            this.plan_comment_draft = Some(selected.clone());
                            this.plan_selection = None;
                            gpui_kit::base::TextSelection::clear(w, cx);
                            this.plan_comment_input.update(cx, |input, cx| input.focus(w, cx));
                            cx.notify();
                        })))
                        .into_any_element(),
                )
            }
            _ => None,
        };
        let comments = self.plan_comments.iter().enumerate().map(|(i, comment)| {
            h().gap(px(8.)).items_start()
                .child(
                    v().flex_1().min_w_0().gap(px(2.))
                        .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("> {}", short_quote(&comment.quote))))
                        .child(div().text_size(px(13.)).text_color(c(theme::TEXT)).child(comment.text.clone())),
                )
                .child(ui::button(SharedString::from(format!("plan-comment-remove-{i}")), "Remove", ButtonKind::Outline, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if i < this.plan_comments.len() {
                        this.plan_comments.remove(i);
                    }
                    cx.notify();
                })))
        });
        v().gap(px(8.))
            .when(!self.plan_comments.is_empty(), |el| {
                el.child(
                    v().id("plan-comments").gap(px(8.)).max_h(px(180.)).overflow_y_scroll()
                        .child(ui::eyebrow("COMMENTS"))
                        .children(comments),
                )
            })
            .children(offer)
            .into_any_element()
    }
}
