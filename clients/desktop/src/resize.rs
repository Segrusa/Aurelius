//! Side panels the user can widen or narrow by dragging their inner edge.
//! Each panel's width is kept in ui.json; a double-click on the edge resets it.

use crate::app::AppView;
use crate::theme::{self, c};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;

/// The side of the panel that faces the content, where the handle sits.
#[derive(Clone, Copy, PartialEq)]
pub enum Edge {
    Left,
    Right,
}

/// What is being dragged: one panel's edge.
pub struct PanelDrag(&'static str);

/// Nothing follows the mouse while resizing.
struct NoPreview;

impl Render for NoPreview {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        Empty
    }
}

const MIN_WIDTH: f32 = 160.;
/// At most this share of the window, so the content keeps room.
const MAX_SHARE: f32 = 0.75;

impl AppView {
    pub fn panel_width(&self, key: &str, default: f32) -> f32 {
        self.panel_widths.get(key).copied().unwrap_or(default)
    }

    fn set_panel_width(&mut self, key: &'static str, width: Option<f32>, cx: &mut Context<Self>) {
        match width {
            Some(w) => self.panel_widths.insert(key.to_string(), w),
            None => self.panel_widths.remove(key),
        };
        cx.notify();
        // Written once the drag settles, not on every mouse move.
        let widths = self.panel_widths.clone();
        self.panel_save = Some(cx.spawn(async move |_, cx| {
            cx.background_executor().timer(Duration::from_millis(400)).await;
            crate::app::set_ui_value("panel_widths", serde_json::json!(widths));
        }));
    }

    /// `panel` (which should fill its frame) in a frame of the remembered width,
    /// with a drag handle on `edge`.
    pub fn resizable(&self, key: &'static str, default: f32, edge: Edge, panel: impl IntoElement, cx: &mut Context<Self>) -> Stateful<Div> {
        let handle = div()
            .id(SharedString::from(format!("{key}-resize")))
            .absolute()
            .top_0()
            .bottom_0()
            .w(px(8.))
            .map(|el| match edge {
                Edge::Right => el.right(px(-4.)),
                Edge::Left => el.left(px(-4.)),
            })
            .cursor(CursorStyle::ResizeLeftRight)
            .hover(|s| s.bg(Hsla { a: 0.5, ..c(theme::BORDER_STRONG) }))
            .on_drag(PanelDrag(key), |_, _, _, cx| cx.new(|_| NoPreview))
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                if e.click_count() == 2 {
                    this.set_panel_width(key, None, cx);
                }
            }));
        div()
            .id(SharedString::from(format!("{key}-frame")))
            .relative()
            // A basis, not a fixed width: a width remembered in a wider window gives way when
            // the window narrows instead of pushing the panel past its edge.
            .flex_basis(px(self.panel_width(key, default)))
            .flex_shrink(1.)
            .min_w(px(MIN_WIDTH))
            .max_w(relative(MAX_SHARE))
            .h_full()
            .child(panel)
            // Painted above the neighbouring content so both halves of the handle catch the mouse.
            .child(deferred(handle))
            .on_drag_move(cx.listener(move |this, e: &DragMoveEvent<PanelDrag>, window, cx| {
                if e.drag(cx).0 != key {
                    return;
                }
                let x = e.event.position.x;
                let width = f32::from(match edge {
                    Edge::Right => x - e.bounds.left(),
                    Edge::Left => e.bounds.right() - x,
                });
                let max = (f32::from(window.viewport_size().width) * MAX_SHARE).max(MIN_WIDTH);
                this.set_panel_width(key, Some(width.clamp(MIN_WIDTH, max)), cx);
            }))
    }
}
