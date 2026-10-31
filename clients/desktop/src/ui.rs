//! Small building blocks shared by all screens.

use crate::theme::{self, Fonts, c};
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub fn serif(cx: &App) -> SharedString {
    Fonts::get(cx).serif.clone()
}

pub fn mono(cx: &App) -> SharedString {
    Fonts::get(cx).mono.clone()
}

/// The stone-cut "A" mark (design option 2d), drawn as a filled path.
pub fn logo(size: f32, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let s = bounds.size.width / 120.;
            let o = bounds.origin;
            let p = |x: f32, y: f32| point(o.x + s * x, o.y + s * y);
            let mut path = PathBuilder::fill();
            path.move_to(p(60., 14.));
            path.line_to(p(104., 106.));
            path.line_to(p(82., 106.));
            path.line_to(p(60., 56.));
            path.line_to(p(38., 106.));
            path.line_to(p(16., 106.));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, c(color));
            }
        },
    )
    .size(px(size))
    .flex_none()
}

/// Rotated-square marker used for decisions and items that need you.
pub fn diamond(size: f32, color: u32, filled: bool) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let w = bounds.size.width;
            let o = bounds.origin;
            let half = w / 2.;
            let mut path = if filled { PathBuilder::fill() } else { PathBuilder::stroke(px(1.3)) };
            path.move_to(point(o.x + half, o.y + px(0.7)));
            path.line_to(point(o.x + w - px(0.7), o.y + half));
            path.line_to(point(o.x + half, o.y + w - px(0.7)));
            path.line_to(point(o.x + px(0.7), o.y + half));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, c(color));
            }
        },
    )
    .size(px(size))
    .flex_none()
}

/// Fold chevron for collapsible sections: pointing down when open, right when
/// folded. Drawn as a path so it sits centred, unlike the "⌄" glyph.
pub fn chevron(open: bool, size: f32, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let s = bounds.size.width / 10.;
            let o = bounds.origin;
            let p = |x: f32, y: f32| if open { point(o.x + s * x, o.y + s * y) } else { point(o.x + s * y, o.y + s * x) };
            let mut path = PathBuilder::stroke(px(1.3));
            path.move_to(p(2., 3.5));
            path.line_to(p(5., 6.5));
            path.line_to(p(8., 3.5));
            if let Ok(path) = path.build() {
                window.paint_path(path, c(color));
            }
        },
    )
    .size(px(size))
    .flex_none()
}

/// "Answer", "Review plan"…: what a conversation in a list waits for from the user.
pub fn needs_you_tag(label: &'static str, color: u32) -> Div {
    h().flex_none().gap(px(5.)).text_size(px(11.)).text_color(c(color)).child(diamond(7., color, true)).child(label)
}

/// Check mark, drawn as a path so it sits centred at small sizes.
/// A push-pin: outlined when `filled` is false, solid when true.
pub fn pin(filled: bool, size: f32, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let s = bounds.size.width / 10.;
            let o = bounds.origin;
            let p = |x: f32, y: f32| point(o.x + s * x, o.y + s * y);
            let mut head = if filled { PathBuilder::fill() } else { PathBuilder::stroke(px(1.2)) };
            head.move_to(p(3., 0.8));
            head.line_to(p(7., 0.8));
            head.line_to(p(7., 3.8));
            head.line_to(p(8.6, 5.8));
            head.line_to(p(1.4, 5.8));
            head.line_to(p(3., 3.8));
            head.close();
            if let Ok(path) = head.build() {
                window.paint_path(path, c(color));
            }
            let mut needle = PathBuilder::stroke(px(1.2));
            needle.move_to(p(5., 5.8));
            needle.line_to(p(5., 9.4));
            if let Ok(path) = needle.build() {
                window.paint_path(path, c(color));
            }
        },
    )
    .size(px(size))
    .flex_none()
}

pub fn check(size: f32, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let s = bounds.size.width / 10.;
            let o = bounds.origin;
            let mut path = PathBuilder::stroke(px(1.5));
            path.move_to(point(o.x + s * 1.8, o.y + s * 5.2));
            path.line_to(point(o.x + s * 4.2, o.y + s * 7.5));
            path.line_to(point(o.x + s * 8.4, o.y + s * 2.8));
            if let Ok(path) = path.build() {
                window.paint_path(path, c(color));
            }
        },
    )
    .size(px(size))
    .flex_none()
}

/// How a task stands, as the user reads it at a glance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskMark {
    Done,
    Failed,
    Cancelled,
    NeedsYou,
    Working,
    /// Working, and compacting the conversation's context first.
    Compacting,
    Queued,
    InLine,
    Scheduled,
    Waiting,
    /// Waiting for the Claude usage limit to reset: calm, never a failure.
    /// With the reset time when the server knows it.
    LimitReset(Option<i64>),
    Draft,
}

/// What a run shows while it compacts the conversation's context.
pub const COMPACTING: &str = "Context comprimeren…";

/// Shown wherever a quota wait shows; `limit_wait` adds the reset time.
pub const LIMIT_WAIT: &str = "Waiting for limit reset";

/// Not finished, or finished with a follow-up run that goes or waits (`live_run`).
pub fn is_active(t: &aurelius_protocol::TaskView) -> bool {
    !t.status.is_terminal() || t.live_run.is_some()
}

/// A queued task that does not start now (the server says 'in_line' or 'scheduled'):
/// it is waiting, not active. Older servers never send a reason for queued tasks.
pub fn is_waiting(t: &aurelius_protocol::TaskView) -> bool {
    use aurelius_domain::task::TaskStatus::*;
    t.status == Waiting || (t.status == Queued && t.waiting_reason.is_some())
}

/// "In line", "Scheduled" or "Scheduled · starts Today 14:05" for a queued task that does not start now.
pub fn queued_wait(t: &aurelius_protocol::TaskView) -> Option<String> {
    if t.status != aurelius_domain::task::TaskStatus::Queued {
        return None;
    }
    Some(match (t.waiting_reason.as_deref()?, t.starts_at_ms) {
        ("scheduled", Some(ms)) => format!("Scheduled · starts {}", fmt_when(ms)),
        ("scheduled", None) => "Scheduled".into(),
        _ => "In line".into(),
    })
}

impl TaskMark {
    pub fn of(t: &aurelius_protocol::TaskView) -> Self {
        use aurelius_domain::task::TaskStatus::*;
        // A finished task whose follow-up run goes or waits is working or queued, not done.
        if t.status.is_terminal() {
            match t.live_run {
                Some(aurelius_domain::views::LiveRun::Running) => return TaskMark::Working,
                Some(aurelius_domain::views::LiveRun::Queued) => return TaskMark::Queued,
                None => {}
            }
        }
        match t.status {
            Succeeded => TaskMark::Done,
            Failed => TaskMark::Failed,
            Cancelled => TaskMark::Cancelled,
            _ if t.pending_approvals > 0 => TaskMark::NeedsYou,
            Waiting if t.waiting_reason.as_deref() == Some("input") => TaskMark::NeedsYou,
            Waiting if t.waiting_reason.as_deref() == Some("quota") => TaskMark::LimitReset(t.waiting_until_ms),
            Waiting => TaskMark::Waiting,
            Running | Review => TaskMark::Working,
            Queued if t.waiting_reason.as_deref() == Some("scheduled") => TaskMark::Scheduled,
            Queued if t.waiting_reason.is_some() => TaskMark::InLine,
            Queued => TaskMark::Queued,
            Draft => TaskMark::Draft,
        }
    }

    /// `of`, with the run's live state: a working task that compacts its context says so.
    pub fn of_live(t: &aurelius_protocol::TaskView, compacting: bool) -> Self {
        match Self::of(t) {
            TaskMark::Working if compacting => TaskMark::Compacting,
            mark => mark,
        }
    }

    pub fn label(self) -> String {
        match self {
            TaskMark::Done => "Done",
            TaskMark::Failed => "Failed",
            TaskMark::Cancelled => "Cancelled",
            TaskMark::NeedsYou => "Needs you",
            TaskMark::Working => "Working",
            TaskMark::Compacting => COMPACTING,
            TaskMark::Queued => "Queued",
            TaskMark::InLine => "In line",
            TaskMark::Scheduled => "Scheduled",
            TaskMark::Waiting => "Waiting",
            TaskMark::LimitReset(until) => return limit_wait(until),
            TaskMark::Draft => "Draft",
        }
        .into()
    }

    pub fn is_finished(self) -> bool {
        matches!(self, TaskMark::Done | TaskMark::Failed | TaskMark::Cancelled)
    }
}

/// Compact chip for a task's state: a green check + "Done" for a finished task,
/// its own mark for failed, cancelled, needs-you and working.
pub fn task_mark(mark: TaskMark, id: impl Into<ElementId>) -> Div {
    let chip = |fg: u32, bg: u32, border: u32| {
        h().flex_none().h(px(18.)).px(px(6.)).gap(px(4.)).rounded(px(4.)).border_1().border_color(c(border)).bg(c(bg)).text_size(px(10.5)).text_color(c(fg))
    };
    match mark {
        TaskMark::Done => chip(theme::ACCENT_TEXT, theme::ACCENT_HALO, theme::ATTENTION_BORDER).child(check(10., theme::ACCENT_TEXT)).child("Done"),
        TaskMark::Failed => chip(theme::NEGATIVE, theme::BG, theme::NEGATIVE).child(div().text_size(px(10.)).child("✕")).child("Failed"),
        TaskMark::Cancelled => chip(theme::MUTED, theme::BG, theme::BORDER_STRONG).child(div().w(px(7.)).h(px(1.)).bg(c(theme::MUTED))).child("Cancelled"),
        TaskMark::NeedsYou => chip(theme::ACCENT_TEXT, theme::ATTENTION_BG, theme::ATTENTION_BORDER).child(diamond(7., theme::ACCENT_TEXT, true)).child("Needs you"),
        TaskMark::Working => chip(theme::TEXT_2, theme::BG, theme::BORDER).child(pulse_dot(id, 6., theme::ACCENT)).child("Working"),
        TaskMark::Compacting => chip(theme::ACCENT_TEXT, theme::ATTENTION_BG, theme::ATTENTION_BORDER).child(pulse_dot(id, 6., theme::ACCENT)).child(COMPACTING),
        TaskMark::Queued | TaskMark::InLine | TaskMark::Scheduled | TaskMark::Waiting | TaskMark::LimitReset(_) => chip(theme::TEXT_2, theme::BG, theme::BORDER).child(ring(6., theme::TEXT_2)).child(mark.label()),
        TaskMark::Draft => chip(theme::MUTED, theme::BG, theme::BORDER).child(ring(6., theme::MUTED)).child("Draft"),
    }
}

/// Back / forward arrow (shaft with a sharp head), drawn as a filled path on a 24-unit grid.
pub fn nav_arrow(forward: bool, size: f32, color: u32) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            let s = bounds.size.width / 24.;
            let o = bounds.origin;
            let p = |x: f32, y: f32| point(o.x + s * if forward { 24. - x } else { x }, o.y + s * y);
            let mut path = PathBuilder::fill();
            path.move_to(p(20., 11.));
            path.line_to(p(7.83, 11.));
            path.line_to(p(13.42, 5.41));
            path.line_to(p(12., 4.));
            path.line_to(p(4., 12.));
            path.line_to(p(12., 20.));
            path.line_to(p(13.42, 18.59));
            path.line_to(p(7.83, 13.));
            path.line_to(p(20., 13.));
            path.close();
            if let Ok(path) = path.build() {
                window.paint_path(path, c(color));
            }
        },
    )
    .size(px(size))
    .flex_none()
}

pub fn dot(size: f32, color: u32) -> Div {
    div().size(px(size)).flex_none().rounded_full().bg(c(color))
}

pub fn ring(size: f32, color: u32) -> Div {
    div().size(px(size)).flex_none().rounded_full().border_1().border_color(c(color))
}

/// Employee monogram: a serif initial in a hairline rounded square. No faces. A
/// board member is a circle (`member_monogram`), so human and agent differ by
/// shape, not only by colour (P-31).
pub fn monogram(name: &str, size: f32, cx: &App) -> Div {
    let initial: String = name.chars().next().map(|ch| ch.to_uppercase().collect()).unwrap_or_default();
    div()
        .size(px(size))
        .flex_none()
        .rounded(px(size * 0.28))
        .border_1()
        .border_color(c(theme::BORDER_STRONG))
        .flex()
        .items_center()
        .justify_center()
        .font_family(serif(cx))
        .text_size(px(size * 0.47))
        .child(initial)
}

/// Eight colours for board members, each readable on the panels (4.5:1, P-31).
pub const MEMBER_COLOURS: [u32; 8] = [0x8FB6E0, 0xE0A87A, 0x9CCB8F, 0xD9A0C8, 0xC9B66F, 0x8FCFC6, 0xB9A6E6, 0xE39C9C];

/// A board member's fixed colour: the same name, the same colour, on every device.
pub fn member_colour(name: &str) -> u32 {
    let hash = name.to_lowercase().bytes().fold(2_166_136_261u32, |h, b| (h ^ b as u32).wrapping_mul(16_777_619));
    MEMBER_COLOURS[(hash % MEMBER_COLOURS.len() as u32) as usize]
}

/// Board member monogram: the initial in a circle of their own colour (P-31).
pub fn member_monogram(name: &str, size: f32, cx: &App) -> Div {
    let initial: String = name.chars().next().map(|ch| ch.to_uppercase().collect()).unwrap_or_default();
    let colour = member_colour(name);
    div()
        .size(px(size))
        .flex_none()
        .rounded_full()
        .border_1()
        .border_color(c(colour))
        .text_color(c(colour))
        .flex()
        .items_center()
        .justify_center()
        .font_family(serif(cx))
        .text_size(px(size * 0.47))
        .child(initial)
}

/// Uppercase section label ("PROJECTS", "TODAY").
pub fn eyebrow(text: impl Into<SharedString>) -> Div {
    div().text_size(px(11.)).text_color(c(theme::MUTED)).child(text.into())
}

/// `eyebrow` followed by its info icon (`id` unique on the screen).
pub fn eyebrow_with_info(id: impl Into<ElementId>, text: impl Into<SharedString>, about: impl Into<SharedString>) -> Div {
    h().gap(px(6.)).items_center().child(eyebrow(text)).child(info(id, about))
}

/// Shortcut hints are hidden unless the user chose to always see them (Settings → App).
/// Everything stays clickable.
#[derive(Default, Clone, Copy)]
pub struct Hints {
    pub always: bool,
}

impl Global for Hints {}

pub fn hints_visible(cx: &App) -> bool {
    cx.try_global::<Hints>().is_none_or(|h| h.always)
}

/// Keyboard shortcut chip next to an action; hidden unless hints are shown.
pub fn kbd(keys: &str, cx: &App) -> Div {
    if !hints_visible(cx) {
        return h_flex().flex_none();
    }
    kbd_always(keys, cx)
}

/// Shortcut chip that is always visible (shortcut overview, command palette).
pub fn kbd_always(keys: &str, cx: &App) -> Div {
    h_flex().gap(px(3.)).flex_none().children(crate::keymap::label(keys).split(' ').map(|part| {
        div()
            .px(px(5.))
            .h(px(17.))
            .min_w(px(17.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .border_1()
            .border_color(c(theme::BORDER_STRONG))
            .bg(c(theme::SURFACE_2))
            .text_color(c(theme::TEXT_2))
            .font_family(mono(cx))
            .text_size(px(10.5))
            .child(part.to_string())
    }))
}

#[derive(Clone, Copy, PartialEq)]
pub enum ButtonKind {
    Outline,
    Primary,
}

/// Bordered button with an optional shortcut hint. The click handler must
/// dispatch the same action the shortcut does.
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, kind: ButtonKind, hint: Option<Div>) -> Stateful<Div> {
    let label: SharedString = label.into();
    h_flex()
        .id(id)
        // Screen readers announce our own buttons by role and name (A17).
        .role(Role::Button)
        .aria_label(label.clone())
        .h(px(32.))
        .px(px(14.))
        .gap(px(8.))
        .justify_center()
        .rounded(px(6.))
        .text_size(px(13.))
        .cursor_pointer()
        .map(|el| match kind {
            ButtonKind::Primary => el
                .bg(c(theme::ACCENT))
                .text_color(c(theme::BG))
                .font_weight(FontWeight::MEDIUM)
                .hover(|s| s.bg(c(theme::FOCUS_RING))),
            ButtonKind::Outline => el
                .border_1()
                .border_color(c(theme::BORDER))
                .hover(|s| s.bg(c(theme::SURFACE_2))),
        })
        .child(label)
        .when_some(hint, |el, hint| el.child(hint))
}

/// Visible label for sample content.
pub fn preview_tag(cx: &App) -> Div {
    div()
        .flex_none()
        .px(px(6.))
        .py(px(1.))
        .rounded(px(4.))
        .border_1()
        .border_color(c(theme::BORDER_STRONG))
        .text_size(px(10.5))
        .font_family(mono(cx))
        .text_color(c(theme::MUTED))
        .child("Sample")
}

/// Clickable action with its shortcut: the label is always there, the key
/// chip only when hints are shown. Clicking does exactly what the key does.
pub fn hint(id: impl Into<ElementId>, keys: &str, label: impl Into<SharedString>, action: fn() -> Box<dyn Action>, cx: &App) -> Stateful<Div> {
    let label: SharedString = label.into();
    h().id(id)
        .role(Role::Button)
        .aria_label(label.clone())
        .gap(px(5.))
        .px(px(6.))
        .py(px(2.))
        .rounded(px(4.))
        .cursor_pointer()
        .text_color(c(theme::TEXT_2))
        .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
        .on_click(move |_, window, cx| window.dispatch_action(action(), cx))
        .child(kbd(keys, cx))
        .child(label)
}

/// On/off switch. Clicking dispatches `action`, the same one its key fires,
/// so the state shown always comes from the app, not from the click.
pub fn toggle(id: impl Into<ElementId>, on: bool, label: impl Into<SharedString>, action: fn() -> Box<dyn Action>) -> Stateful<Div> {
    let label: SharedString = label.into();
    h().id(id)
        .role(Role::Button)
        .aria_label(SharedString::from(format!("{label}: {}", if on { "on" } else { "off" })))
        .flex_none()
        .w(px(34.))
        .h(px(18.))
        .p(px(2.))
        .rounded_full()
        .cursor_pointer()
        .when(on, |el| el.justify_end().bg(c(theme::ACCENT)).hover(|s| s.bg(c(theme::FOCUS_RING))))
        .when(!on, |el| el.bg(c(theme::SURFACE_2)).border_1().border_color(c(theme::BORDER_STRONG)).hover(|s| s.border_color(c(theme::TEXT_2))))
        .on_click(move |_, window, cx| window.dispatch_action(action(), cx))
        .child(div().size(px(if on { 14. } else { 12. })).rounded_full().bg(c(if on { theme::BG } else { theme::TEXT_2 })))
}

/// Keyboard-only navigation hint (e.g. ↑ ↓ select): nothing at all when hints are hidden.
pub fn key_hint(keys: &[&str], label: impl Into<SharedString>, cx: &App) -> Div {
    if !hints_visible(cx) {
        return div();
    }
    h().gap(px(5.)).children(keys.iter().map(|k| kbd_always(k, cx))).child(label.into())
}

/// ✕ that closes the current panel or dialog (same as Esc).
pub fn close_button(cx: &App) -> Stateful<Div> {
    h().id("overlay-close")
        .role(Role::Button)
        .aria_label("Close")
        .gap(px(6.))
        .px(px(7.))
        .h(px(26.))
        .rounded(px(5.))
        .cursor_pointer()
        .text_size(px(13.))
        .text_color(c(theme::TEXT))
        .hover(|s| s.bg(c(theme::SURFACE_2)))
        .on_click(|_, window, cx| window.dispatch_action(Box::new(crate::keymap::Back), cx))
        .tooltip(tip("Close", Some("escape")))
        .child(kbd("escape", cx))
        .child("✕")
}

/// The "⋯" button that opens an item's menu (also on right-click).
pub fn more_button(id: impl Into<ElementId>) -> Stateful<Div> {
    // Three drawn dots: a "⋯" glyph sits on the text baseline, not the centre.
    icon_button(id).tooltip(tip("More actions", None)).child(h().gap(px(3.)).children((0..3).map(|_| div().size(px(3.)).rounded_full().bg(c(theme::TEXT)))))
}

/// Hover tooltip for a button without a visible label: its name, plus the
/// shortcut chip when it has one. Use as `.tooltip(ui::tip("Settings", Some("secondary-,")))`.
pub fn tip(label: impl Into<SharedString>, keys: Option<&'static str>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let label: SharedString = label.into();
    move |window, cx| {
        let label = label.clone();
        gpui_kit::component::tooltip::Tooltip::element(move |_, cx| h().gap(px(8.)).child(label.clone()).when_some(keys, |el, keys| el.child(kbd_always(keys, cx))))
            .bg(c(theme::SURFACE_2))
            .border_color(c(theme::BORDER_STRONG))
            .text_color(c(theme::TEXT))
            .text_size(px(12.))
            .build(window, cx)
    }
}

/// Compact glyph button for a message action (Copy, Trace, Retry, Context, Stop): the name and
/// shortcut sit in the tooltip and the aria label. With `action`, a click does what the key does.
pub fn action_icon(id: impl Into<ElementId>, glyph: &'static str, label: impl Into<SharedString>, keys: Option<&'static str>, action: Option<fn() -> Box<dyn Action>>) -> Stateful<Div> {
    let label: SharedString = label.into();
    icon_button(id)
        .size(px(24.))
        .line_height(px(24.))
        .text_size(px(14.))
        .text_color(c(theme::MUTED))
        .hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
        .aria_label(label.clone())
        .tooltip(tip(label, keys))
        .when_some(action, |el, action| el.on_click(move |_, window, cx| window.dispatch_action(action(), cx)))
        .child(glyph)
}

/// Small square button for a glyph (⋯, ⚙).
pub fn icon_button(id: impl Into<ElementId>) -> Stateful<Div> {
    div()
        .id(id)
        .role(Role::Button)
        .aria_label("More actions")
        .flex_none()
        .size(px(28.))
        .flex()
        .items_center()
        .justify_center()
        .line_height(px(28.))
        .rounded(px(5.))
        .cursor_pointer()
        .text_size(px(15.))
        .text_color(c(theme::TEXT))
        .hover(|s| s.bg(c(theme::SURFACE_2)))
}

/// Text field with room for descenders (g, j, y). The component's medium
/// size is 32 px high with 8 px padding, leaving 16 px for a 20 px line, so
/// the bottom of g, j and y was cut off.
pub fn input(state: &Entity<gpui_kit::component::input::InputState>) -> gpui_kit::component::input::Input {
    gpui_kit::component::input::Input::new(state).h(px(36.)).py(px(4.)).line_height(px(22.))
}

/// A small panel icon: a frame with its left column filled when the sidebar shows.
pub fn sidebar_glyph(open: bool) -> Div {
    h().w(px(15.))
        .h(px(12.))
        .rounded(px(2.))
        .border_1()
        .border_color(c(theme::TEXT))
        .child(div().w(px(4.)).h_full().when(open, |el| el.bg(c(theme::TEXT))).border_r_1().border_color(c(theme::TEXT)))
}

/// Hairline divider.
pub fn hr() -> Div {
    div().h(px(1.)).w_full().bg(c(theme::BORDER))
}

/// "Employee default (Opus 5.5 · High)" becomes "Opus 5.5 (default)": the form chips and menus show this short form.
pub fn short_default(label: &str) -> String {
    match label.strip_prefix("Employee default (").and_then(|r| r.strip_suffix(')')) {
        Some(inner) => format!("{} (default)", inner.split(" · ").next().unwrap_or(inner)),
        None => label.to_string(),
    }
}

/// The one trigger of every dropdown: a pill with a border and a caret; `set` marks a value that was
/// chosen, `open` the dropdown that is showing. Callers add the children and the click.
pub fn dropdown_trigger(id: impl Into<ElementId>, height: f32, set: bool, open: bool) -> Stateful<Div> {
    h().id(id)
        .role(Role::ComboBox)
        .relative()
        .h(px(height))
        .min_w_0()
        .px(px(12.))
        .gap(px(6.))
        .rounded(px(height / 2.))
        .border_1()
        .border_color(c(if open { theme::ACCENT } else if set { theme::BORDER_STRONG } else { theme::BORDER }))
        .when(open, |el| el.bg(c(theme::SURFACE_2)))
        .cursor_pointer()
        .text_size(px(12.5))
        .text_color(c(if set || open { theme::TEXT } else { theme::TEXT_2 }))
        .hover(|s| s.border_color(c(theme::BORDER_STRONG)).bg(c(theme::SURFACE_2)))
}

/// The caret every dropdown trigger ends with.
pub fn caret() -> Div {
    div().flex_none().text_size(px(9.)).text_color(c(theme::MUTED)).child("▾")
}

/// Records where a trigger is, so its popup can sit right under (or above) it and be as wide.
pub fn track_bounds(bounds: std::rc::Rc<std::cell::RefCell<std::collections::HashMap<&'static str, Bounds<Pixels>>>>, id: &'static str) -> impl IntoElement {
    canvas(move |b, _, _| { bounds.borrow_mut().insert(id, b); }, |_, _, _, _| {}).absolute().top_0().left_0().size_full()
}

/// Selection outline for keyboard navigation.
pub fn selectable(el: Stateful<Div>, selected: bool) -> Stateful<Div> {
    el.border_1().border_color(if selected { c(theme::FOCUS_RING) } else { transparent_black() })
}

pub fn v() -> Div {
    v_flex()
}

pub fn h() -> Div {
    h_flex()
}

/// The growing text cell of a row: takes the free width and cuts long text with "…" instead of pushing the row wider.
pub fn row_title() -> Div {
    div().flex_1().min_w_0().truncate()
}

/// "Thinking" indicator: three dots rising and glowing in a wave.
pub fn thinking(id: &'static str) -> Div {
    h().h(px(18.)).gap(px(5.)).items_center().children((0..3usize).map(move |i| {
        div()
            .relative()
            .size(px(7.))
            .rounded_full()
            .bg(c(theme::ACCENT))
            .with_animation(
                (id, i),
                Animation::new(std::time::Duration::from_millis(1100)).repeat(),
                move |el, t| {
                    let phase = (t - i as f32 * 0.16).rem_euclid(1.0);
                    let wave = (phase * std::f32::consts::TAU).sin().max(0.0);
                    el.top(px(-4.0 * wave)).opacity(0.3 + 0.7 * wave)
                },
            )
    }))
}

/// A dot that breathes while work is in progress.
pub fn pulse_dot(id: impl Into<ElementId>, size: f32, color: u32) -> AnyElement {
    let id = id.into();
    looping(id.clone(), (u64::from(size.to_bits()) << 32) | u64::from(color), size, move || {
        dot(size, color)
            .with_animation(id.clone(), Animation::new(std::time::Duration::from_millis(1400)).repeat(), |el, t| {
                el.opacity(0.35 + 0.65 * (1.0 - (2.0 * t - 1.0).abs()))
            })
            .into_any_element()
    })
}

/// An endless animation (`size` px square) as a small cached view of its own, so its frames
/// re-render only it: inline, each frame notifies the screen's view, and with it the cached
/// conversation pane that observes AppView. `key`: what `content` shows besides time.
pub fn looping(id: impl Into<ElementId>, key: u64, size: f32, content: impl Fn() -> AnyElement + 'static) -> AnyElement {
    Looping { id: id.into(), key: (key, theme::is_light()), size, content: std::rc::Rc::new(content) }.into_any_element()
}

#[derive(IntoElement)]
struct Looping {
    id: ElementId,
    key: (u64, bool),
    size: f32,
    content: std::rc::Rc<dyn Fn() -> AnyElement>,
}

struct LoopingView {
    key: (u64, bool),
    content: std::rc::Rc<dyn Fn() -> AnyElement>,
}

impl Render for LoopingView {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        (self.content)()
    }
}

impl RenderOnce for Looping {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        // Kept in element state, not `use_keyed_state`: that one forwards every notify to the parent view.
        let view = window.with_global_id(self.id, |id, window| {
            window.with_element_state(id, |view: Option<Entity<LoopingView>>, _| {
                // A new key is a new view: a cached one would keep showing the old content.
                let view = view.filter(|v| v.read(cx).key == self.key)
                    .unwrap_or_else(|| cx.new(|_| LoopingView { key: self.key, content: self.content }));
                (view.clone(), view)
            })
        });
        view.cached(StyleRefinement::default().size(px(self.size)).flex_none())
    }
}

/// `pulse_dot` for the cached conversation pane, where an endless animation would rebuild
/// every message each frame: it blinks on the repaints a run makes anyway (streamed text, the 1 s tick).
pub fn blink_dot(size: f32, color: u32) -> Div {
    let on = crate::screens::chat::now_ms() / 1000 % 2 == 0;
    dot(size, color).opacity(if on { 1.0 } else { 0.4 })
}

/// `thinking` for the conversation pane: the lit dot steps on those repaints instead of animating.
pub fn thinking_steps() -> Div {
    let lit = (crate::screens::chat::now_ms() / 1000 % 3) as usize;
    h().h(px(18.)).gap(px(5.)).items_center()
        .children((0..3usize).map(move |i| div().size(px(7.)).rounded_full().bg(c(theme::ACCENT)).opacity(if i == lit { 1.0 } else { 0.3 })))
}

/// Entrance animation: fade in while rising a few pixels (ease-out).
/// `index` staggers list items (capped so long lists stay quick).
pub fn rise<E: IntoElement + Styled + 'static>(el: E, id: impl Into<ElementId>, index: usize) -> AnimationElement<E> {
    let delay = index.min(8) as f32 * 45.0;
    let base = 340.0;
    let total = base + delay;
    el.with_animation(id, Animation::new(std::time::Duration::from_millis(total as u64)), move |el, t| {
        let p = ((t * total - delay) / base).clamp(0.0, 1.0);
        let e = 1.0 - (1.0 - p).powi(4);
        el.opacity(e).relative().top(px((1.0 - e) * 10.0))
    })
}

/// Fade in only (for elements whose position must not move).
pub fn fade_in<E: IntoElement + Styled + 'static>(el: E, id: impl Into<ElementId>, ms: u64) -> AnimationElement<E> {
    el.with_animation(id, Animation::new(std::time::Duration::from_millis(ms)), |el, t| el.opacity(1.0 - (1.0 - t).powi(3)))
}

/// Gentle continuous breathing for "something is going on" markers.
pub fn breathe<E: IntoElement + Styled + 'static>(el: E, id: impl Into<ElementId>) -> AnimationElement<E> {
    el.with_animation(id, Animation::new(std::time::Duration::from_millis(1600)).repeat(), |el, t| {
        el.opacity(0.45 + 0.55 * (1.0 - (2.0 * t - 1.0).abs()))
    })
}

/// Compact token count: 950 → "950", 12_400 → "12,4k", 1_250_000 → "1,25M".
pub fn fmt_tokens(n: u64) -> String {
    match n {
        0..=999 => n.to_string(),
        1_000..=999_999 => format!("{:.1}k", n as f64 / 1_000.0).replace(".0k", "k"),
        _ => format!("{:.2}M", n as f64 / 1_000_000.0),
    }
}

/// How long something runs: "45s", "2m 26s", "1h 03m".
pub fn fmt_elapsed(ms: i64) -> String {
    let s = (ms.max(0) / 1000) as u64;
    match s {
        0..=59 => format!("{s}s"),
        60..=3599 => format!("{}m {}s", s / 60, s % 60),
        _ => format!("{}h {:02}m", s / 3600, (s % 3600) / 60),
    }
}

/// "2m 26s · 3.9k tokens" for something running since `started_at_ms`.
pub fn fmt_progress(started_at_ms: i64, output_tokens: u64) -> String {
    let elapsed = fmt_elapsed(now_ms() - started_at_ms);
    if output_tokens == 0 { elapsed } else { format!("{elapsed} · {} tokens", fmt_tokens(output_tokens)) }
}

/// `fmt_progress` of a running thing; "Context comprimeren… · 12s" while it compacts.
pub fn fmt_run(p: &aurelius_protocol::RunProgress) -> String {
    let progress = fmt_progress(p.started_at_ms, p.output_tokens);
    if p.compacting { format!("{COMPACTING} · {progress}") } else { progress }
}

pub fn now_ms() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0)
}

/// Cost in dollars from micro-USD.
pub fn fmt_cost(micro: i64) -> String {
    let dollars = micro as f64 / 1_000_000.0;
    if dollars > 0.0 && dollars < 0.01 { format!("${dollars:.4}") } else { format!("${dollars:.2}") }
}

/// Three small bars: the usage icon.
pub fn usage_glyph(color: u32) -> Div {
    h().h(px(11.)).gap(px(2.)).items_end()
        .child(div().w(px(3.)).h(px(5.)).rounded(px(1.)).bg(c(color)))
        .child(div().w(px(3.)).h(px(11.)).rounded(px(1.)).bg(c(color)))
        .child(div().w(px(3.)).h(px(8.)).rounded(px(1.)).bg(c(color)))
}

/// Small folder icon drawn with divs (tab + body).
pub fn folder_glyph(color: u32) -> Div {
    v().w(px(13.)).h(px(10.))
        .child(div().w(px(6.)).h(px(2.)).rounded_tl(px(1.5)).rounded_tr(px(1.5)).bg(c(color)))
        .child(div().w_full().h(px(8.)).rounded(px(1.5)).border_1().border_color(c(color)))
}

/// "just now", "5m ago", "2h ago" or "3d ago" for `ms` seen at `now_ms`.
pub fn relative_age(ms: i64, now_ms: i64) -> String {
    let secs = (now_ms - ms).max(0) / 1000;
    match secs {
        0..60 => "just now".into(),
        60..3_600 => format!("{}m ago", secs / 60),
        3_600..86_400 => format!("{}h ago", secs / 3_600),
        _ => format!("{}d ago", secs / 86_400),
    }
}

/// The inbox's day section of a timestamp: TODAY, YESTERDAY or EARLIER (local time).
pub fn day_group(ms: i64) -> &'static str {
    let when = fmt_when(ms);
    if when.starts_with("Today") { "TODAY" } else if when.starts_with("Yesterday") { "YESTERDAY" } else { "EARLIER" }
}

/// "Today 14:05", "Yesterday 09:12" or "3 Oct 14:05" (local time).
pub fn fmt_when(ms: i64) -> String {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
        + aurelius_host::local_utc_offset_minutes() * 60;
    let t = ms / 1000 + aurelius_host::utc_offset_minutes_at(ms) * 60;
    let (day, today) = (t.div_euclid(86_400), now.div_euclid(86_400));
    let clock = format!("{:02}:{:02}", t.rem_euclid(86_400) / 3600, t.rem_euclid(3600) / 60);
    match today - day {
        0 => format!("Today {clock}"),
        1 => format!("Yesterday {clock}"),
        _ => format!("{} {clock}", day_month(day)),
    }
}

/// "Waiting for limit reset until 14:05", with the day when that is not today;
/// the plain text when the reset time is unknown.
pub fn limit_wait(until_ms: Option<i64>) -> String {
    limit_wait_as(LIMIT_WAIT, until_ms)
}

/// "your" → "Your" (a sentence's first word).
pub fn capitalised(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or_else(String::new, |f| f.to_uppercase().collect::<String>() + chars.as_str())
}

/// A ticket's quota wait with whose limit it is (ui-ontwerp §5, B12): "Waiting for
/// Alex's limit until 16:00", or "your" for the own; never more of the other's usage.
pub fn limit_wait_for(owner: &str, until_ms: Option<i64>) -> String {
    limit_wait_as(&format!("Waiting for {owner} limit"), until_ms)
}

fn limit_wait_as(text: &str, until_ms: Option<i64>) -> String {
    let Some(ms) = until_ms else { return text.into() };
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs() as i64).unwrap_or(0)
        + aurelius_host::local_utc_offset_minutes() * 60;
    let t = ms / 1000 + aurelius_host::utc_offset_minutes_at(ms) * 60;
    let clock = format!("{:02}:{:02}", t.rem_euclid(86_400) / 3600, t.rem_euclid(3600) / 60);
    if t.div_euclid(86_400) == now.div_euclid(86_400) { format!("{text} until {clock}") } else { format!("{text} until {} {clock}", fmt_day(ms)) }
}

/// "Mon 5 Oct" for the local day that contains `ms`.
pub fn fmt_day(ms: i64) -> String {
    let day = (ms / 1000 + aurelius_host::utc_offset_minutes_at(ms) * 60).div_euclid(86_400);
    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][day.rem_euclid(7) as usize];
    format!("{weekday} {}", day_month(day))
}

/// "5 Oct": civil date from days since 1970-01-01.
fn day_month(day: i64) -> String {
    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    format!("{d} {}", MONTHS[(m - 1) as usize])
}

/// Wraps a scroll area that tracks `scroll` so it shows a visible scrollbar.
/// The caller sizes the frame; the body fills it with `flex_1().min_h_0()`.
pub fn scroll_frame(scroll: &ScrollHandle, body: impl IntoElement) -> Div {
    use gpui_kit::component::scroll::ScrollableElement as _;
    div().relative().flex().flex_col().min_h_0().min_w_0().child(body).vertical_scrollbar(scroll)
}

struct InfoState {
    open: bool,
    focus: FocusHandle,
}

/// Explanation behind a small drawn "i" in a circle. Hover shows it as a tooltip, click, Enter or
/// Space opens it as a popover, Esc closes the popover. It is a tab stop with a label, so it works
/// without a mouse. `id` must be unique on the screen.
pub fn info(id: impl Into<ElementId>, text: impl Into<SharedString>) -> impl IntoElement {
    Info { id: id.into(), text: text.into() }
}

#[derive(IntoElement)]
struct Info {
    id: ElementId,
    text: SharedString,
}

fn flip(state: &Entity<InfoState>, cx: &mut App) {
    state.update(cx, |s, cx| {
        s.open = !s.open;
        cx.notify();
    });
}

impl RenderOnce for Info {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let state = window.use_keyed_state((self.id.clone(), "state"), cx, |_, cx| InfoState { open: false, focus: cx.focus_handle().tab_stop(true) });
        let (open, focus) = {
            let s = state.read(cx);
            (s.open, s.focus.clone())
        };
        let (on_click, on_key, on_esc, outside) = (state.clone(), state.clone(), state.clone(), state);
        let text = self.text.clone();
        let tip_text = self.text;
        let ink = c(if open { theme::TEXT } else { theme::TEXT_2 });
        div()
            .relative()
            .flex_none()
            .child(
                div()
                    .id(self.id)
                    .role(Role::Button)
                    .aria_label(SharedString::from(format!("Info: {text}")))
                    .track_focus(&focus)
                    .key_context(crate::keymap::INFO)
                    .flex_none()
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .cursor_pointer()
                    .hover(|s| s.bg(c(theme::SURFACE_2)))
                    .focus_visible(|s| s.border_1().border_color(c(theme::FOCUS_RING)))
                    .on_click({
                        let focus = focus.clone();
                        move |_, window, cx| {
                            // Inside a clickable row, this click is not the row's.
                            cx.stop_propagation();
                            focus.focus(window, cx);
                            flip(&on_click, cx);
                        }
                    })
                    .on_action(move |_: &crate::keymap::InfoToggle, _, cx| flip(&on_key, cx))
                    .on_action(move |_: &crate::keymap::InfoClose, window, cx| {
                        if on_esc.read(cx).open {
                            flip(&on_esc, cx);
                        } else {
                            window.dispatch_action(Box::new(crate::keymap::Back), cx);
                        }
                    })
                    .when(!open, |el| el.tooltip(wide_tip(tip_text)))
                    .child(
                        // The "i" is drawn (dot and stem): a glyph sits on the text baseline.
                        v().size(px(14.))
                            .rounded_full()
                            .border_1()
                            .border_color(ink)
                            .items_center()
                            .justify_center()
                            .gap(px(1.))
                            .child(div().size(px(2.)).rounded_full().bg(ink))
                            .child(div().w(px(2.)).h(px(4.)).bg(ink)),
                    ),
            )
            .when(open, |el| {
                el.child(div().absolute().top(px(22.)).left_0().child(
                    deferred(
                        anchored().anchor(Anchor::TopLeft).snap_to_window().child(
                            div()
                                .occlude()
                                .w(px(280.))
                                .p(px(10.))
                                .rounded(px(6.))
                                .border_1()
                                .border_color(c(theme::BORDER_STRONG))
                                .bg(c(theme::SURFACE_2))
                                .text_size(px(12.5))
                                .line_height(px(18.))
                                .text_color(c(theme::TEXT))
                                .on_mouse_down_out(move |_, _, cx| {
                                    if outside.read(cx).open {
                                        flip(&outside, cx);
                                    }
                                })
                                .child(text),
                        ),
                    )
                    .with_priority(2),
                ))
            })
    }
}

/// Like `tip`, but for a sentence or two: wraps at a maximum width.
fn wide_tip(text: SharedString) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    move |window, cx| {
        let text = text.clone();
        gpui_kit::component::tooltip::Tooltip::element(move |_, _| div().max_w(px(280.)).child(text.clone()))
            .bg(c(theme::SURFACE_2))
            .border_color(c(theme::BORDER_STRONG))
            .text_color(c(theme::TEXT))
            .text_size(px(12.))
            .build(window, cx)
    }
}

/// A label followed by its ⓘ. The icon id comes from the label, so use a label that is unique on the screen.
pub fn label_with_info(label: impl Into<SharedString>, text: impl Into<SharedString>) -> Div {
    let label: SharedString = label.into();
    h().gap(px(4.)).items_center().child(label.clone()).child(info((ElementId::from("info"), label), text))
}

/// A settings row: label (with ⓘ when `explain` is given) on the left, the control on the right.
pub fn setting_row(label: impl Into<SharedString>, explain: Option<&str>, control: impl IntoElement) -> Div {
    let label: SharedString = label.into();
    let left = match explain {
        Some(text) => label_with_info(label, text.to_string()),
        None => h().child(label),
    };
    h().w_full().gap(px(12.)).items_center().justify_between().child(left.text_size(px(13.)).text_color(c(theme::TEXT))).child(control)
}

#[cfg(test)]
mod tests {
    use super::{MEMBER_COLOURS, member_colour};

    /// P-31: a board member's monogram has one fixed colour of the eight, whatever the device or case.
    #[test]
    fn a_board_member_keeps_one_colour_of_the_palette() {
        assert_eq!(member_colour("alex"), member_colour("Alex"));
        assert!(MEMBER_COLOURS.contains(&member_colour("robin")));
        let distinct: std::collections::HashSet<u32> = MEMBER_COLOURS.into_iter().collect();
        assert_eq!(distinct.len(), 8);
    }
}
