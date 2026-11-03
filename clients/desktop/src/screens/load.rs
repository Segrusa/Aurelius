//! The server's load (protocol 1.10): Settings → Load limits (live meter and
//! editable limits) and the "Server busy" line in a conversation whose answer
//! waits for the machine. Live through `load.changed`.

use crate::app::{AppView, Overlay, Page};
use crate::backend::{self, Connection};
use crate::keymap::*;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::conversation::{MessageRole, MessageStatus};
use aurelius_protocol::{LoadHoldKind, LoadLimits, LoadLimitsUpdate, LoadSnapshot, MessageView};
use gpui_kit::component::input::InputState;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub const LOAD_TAB: usize = 5;
pub const NO_RIGHT: &str = "Changing load limits needs the Approve right on this server.";

/// Settings → Load limits: the limits being edited.
pub struct LoadForm {
    pub max_runs: Entity<InputState>,
    pub cpu_high: Entity<InputState>,
    pub cpu_low: Entity<InputState>,
    /// In GB (the server keeps MB).
    pub mem_min: Entity<InputState>,
    pub enabled: bool,
    /// The limits the fields were filled with; while the fields still say
    /// that, newer limits from the server replace them.
    shown: Option<LoadLimits>,
    /// Fill the fields from the next snapshot (entering the tab, after saving).
    fill: bool,
    pub error: Option<String>,
    pub saving: bool,
}

impl LoadForm {
    pub fn new(window: &mut Window, cx: &mut App) -> Self {
        let field = |placeholder: &'static str, window: &mut Window, cx: &mut App| cx.new(|cx| InputState::new(window, cx).placeholder(placeholder));
        Self {
            max_runs: field("3", window, cx),
            cpu_high: field("75", window, cx),
            cpu_low: field("65", window, cx),
            mem_min: field("2", window, cx),
            enabled: true,
            shown: None,
            fill: true,
            error: None,
            saving: false,
        }
    }

    fn fields(&self) -> [Entity<InputState>; 4] {
        [self.max_runs.clone(), self.cpu_high.clone(), self.cpu_low.clone(), self.mem_min.clone()]
    }

    fn parsed(&self, cx: &App) -> Result<LoadLimits, String> {
        let text = |f: &Entity<InputState>| f.read(cx).value().to_string();
        parse_limits(&text(&self.max_runs), &text(&self.cpu_high), &text(&self.cpu_low), &text(&self.mem_min), self.enabled)
    }
}

/// The banner over a conversation whose run waits; `None` when nothing holds
/// new runs. Uses the live values, so it follows `load.changed`.
pub fn waiting_text(s: &LoadSnapshot) -> Option<String> {
    Some(match s.hold.as_ref()?.kind {
        LoadHoldKind::Cpu => format!("Server busy: CPU {}%, waiting", s.cpu_pct.round() as i64),
        LoadHoldKind::Memory => format!("Server busy: {} free, waiting", gigabytes(s.mem_free_mb)),
        LoadHoldKind::MaxRuns => format!("Server busy: {} active, waiting", if s.running == 1 { "1 run".to_string() } else { format!("{} runs", s.running) }),
        LoadHoldKind::DisabledAll => "Runs paused on the server".to_string(),
    })
}

/// "1.4 GB" (one decimal below 10 GB).
pub fn gigabytes(mb: u64) -> String {
    let gb = mb as f64 / 1024.0;
    if gb < 10.0 { format!("{gb:.1} GB") } else { format!("{} GB", gb.round() as u64) }
}

/// A GB value for the field: "2", "1.5".
fn gb_field(mb: u64) -> String {
    let gb = (mb as f64 / 1024.0 * 10.0).round() / 10.0;
    if gb.fract() == 0.0 { format!("{}", gb as u64) } else { format!("{gb:.1}") }
}

/// The run's reason is not on the desktop (a held run has no answer yet and
/// event notices carry no metadata), so: a message waits for an answer that
/// has not started. The last message is the user's (or a task brief/finding/message
/// the manager answers) and nothing streams.
pub fn awaiting_start(messages: &[MessageView]) -> bool {
    if messages.iter().any(|m| m.role == MessageRole::Assistant && m.status == MessageStatus::Partial) {
        return false;
    }
    match messages.last() {
        Some(m) if m.role == MessageRole::User => true,
        Some(m) => matches!(m.update.as_deref(), Some("brief" | "finding" | "message")),
        None => false,
    }
}

/// The limits as typed, checked like the server does: cpu_low < cpu_high ≤ 100,
/// max_runs ≥ 0 (0 holds every new run).
pub fn parse_limits(max_runs: &str, cpu_high: &str, cpu_low: &str, mem_min_gb: &str, enabled: bool) -> Result<LoadLimits, String> {
    let max_runs: u32 = max_runs.trim().parse().map_err(|_| "Max runs must be a whole number (0 holds every new run).".to_string())?;
    let percent = |text: &str, what: &str| -> Result<u8, String> {
        text.trim().parse::<u8>().ok().filter(|p| *p <= 100).ok_or_else(|| format!("{what} must be a whole percentage from 0 to 100."))
    };
    let cpu_high = percent(cpu_high, "CPU high")?;
    let cpu_low = percent(cpu_low, "CPU low")?;
    if cpu_low >= cpu_high {
        return Err("CPU low must be below CPU high.".into());
    }
    let gb: f64 = mem_min_gb.trim().replace(',', ".").parse().ok().filter(|g: &f64| g.is_finite() && *g >= 0.0)
        .ok_or_else(|| "Min free memory must be a number of GB, for example 2 or 1.5.".to_string())?;
    Ok(LoadLimits { max_runs, cpu_high, cpu_low, mem_min_mb: (gb * 1024.0).round() as u64, enabled })
}

pub fn update_of(l: LoadLimits) -> LoadLimitsUpdate {
    LoadLimitsUpdate { max_runs: Some(l.max_runs), cpu_high: Some(l.cpu_high), cpu_low: Some(l.cpu_low), mem_min_mb: Some(l.mem_min_mb), enabled: Some(l.enabled) }
}

impl AppView {
    /// The server in use knows load (1.10) and gave a snapshot.
    pub fn server_load(&self) -> Option<&LoadSnapshot> {
        let online = matches!(&self.connection, Connection::Online { .. });
        self.load.as_ref().filter(|_| online && self.load_error.is_none())
    }

    fn may_set_load(&self) -> bool {
        matches!(&self.connection, Connection::Online { server, .. } if server.approve)
    }

    pub fn enter_load_tab(&mut self) {
        self.load_form.fill = true;
        self.load_form.error = None;
        let _ = self.requests_handle().send(backend::Request::LoadServerLoad);
    }

    pub fn load_loaded(&mut self, result: Result<LoadSnapshot, String>) {
        match result {
            Ok(snapshot) => {
                self.load = Some(snapshot);
                self.load_error = None;
            }
            Err(error) => {
                self.load = None;
                self.load_error = Some(error);
            }
        }
    }

    pub fn load_saved(&mut self, result: Result<LoadSnapshot, String>, cx: &mut Context<Self>) {
        self.load_form.saving = false;
        match result {
            Ok(snapshot) => {
                self.load = Some(snapshot);
                self.load_form.fill = true;
                self.load_form.error = None;
                self.notify_user("Load limits saved; they apply now.", false, cx);
            }
            Err(error) => {
                self.load_form.error = Some(error.clone());
                self.notify_user(error, true, cx);
            }
        }
    }

    pub fn save_load_limits(&mut self, cx: &mut Context<Self>) {
        if self.screen != Page::Settings || self.settings_tab != LOAD_TAB {
            return;
        }
        let refused = if self.server_load().is_none() {
            Some(self.load_error.clone().unwrap_or_else(|| "The server load is not available yet.".into()))
        } else if !self.may_set_load() {
            Some(NO_RIGHT.into())
        } else {
            None
        };
        let outcome = match refused {
            Some(reason) => Err(reason),
            None => self.load_form.parsed(cx),
        };
        match outcome {
            Ok(limits) => {
                let _ = self.requests_handle().send(backend::Request::SetLoadLimits(update_of(limits)));
                self.load_form.saving = true;
                self.load_form.error = None;
            }
            Err(error) => {
                self.load_form.error = Some(error.clone());
                self.notify_user(error, true, cx);
            }
        }
        cx.notify();
    }

    pub fn toggle_load_enabled(&mut self, cx: &mut Context<Self>) {
        if self.screen == Page::Settings && self.settings_tab == LOAD_TAB {
            self.load_form.enabled = !self.load_form.enabled;
            cx.notify();
        }
    }

    /// Tab / Shift+Tab between the fields; Enter on the page goes to the first.
    pub fn load_move_field(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        // A board dialog shares this key context: Tab walks its own fields.
        if let Some(dialog) = self.board.dialog {
            self.board_dialog_move_field(dialog, forward, window, cx);
            return;
        }
        if self.screen != Page::Settings || self.settings_tab != LOAD_TAB || self.server_load().is_none() {
            return;
        }
        let fields = self.load_form.fields();
        let next = match fields.iter().position(|f| f.read(cx).focus_handle(cx).is_focused(window)) {
            Some(i) if forward => (i + 1) % fields.len(),
            Some(i) => (i + fields.len() - 1) % fields.len(),
            None => 0,
        };
        fields[next].update(cx, |input, cx| input.focus(window, cx));
    }

    /// Esc in a field: back to the page (a second Esc closes Settings).
    pub fn load_leave_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.focus.focus(window, cx);
        cx.notify();
    }

    /// The fields show the server's limits until the user changes them.
    fn fill_load_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(limits) = self.server_load().map(|s| s.limits) else { return };
        let form = &self.load_form;
        let untouched = form.shown.is_some_and(|shown| form.parsed(cx).ok() == Some(shown));
        if !(form.fill || (untouched && form.shown != Some(limits))) {
            return;
        }
        let values = [limits.max_runs.to_string(), limits.cpu_high.to_string(), limits.cpu_low.to_string(), gb_field(limits.mem_min_mb)];
        for (field, value) in self.load_form.fields().into_iter().zip(values) {
            field.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        self.load_form.enabled = limits.enabled;
        self.load_form.shown = Some(limits);
        self.load_form.fill = false;
    }

    /// "Server busy: CPU 82%, waiting" under a conversation whose answer has
    /// not started while the server holds runs.
    pub fn load_banner(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let snapshot = self.server_load().filter(|s| s.held > 0)?;
        let text = waiting_text(snapshot)?;
        if !awaiting_start(&self.messages.as_ref()?.messages) {
            return None;
        }
        Some(
            h().id("load-banner").gap(px(10.)).px(px(12.)).py(px(9.)).rounded(px(8.)).border_1()
                .border_color(c(theme::ATTENTION_BORDER)).bg(c(theme::ATTENTION_BG))
                .text_size(px(12.5)).text_color(c(theme::ACCENT_TEXT))
                .child(ui::blink_dot(7., theme::ACCENT))
                .child(div().flex_1().min_w_0().child(text))
                .child(
                    ui::button("load-banner-limits", "Load limits", ButtonKind::Outline, None).h(px(26.))
                        .on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                            this.open_settings(&OpenSettings, w, cx);
                            this.set_settings_tab(LOAD_TAB, cx);
                        })),
                )
                .into_any_element(),
        )
    }

    /// Settings → Load limits.
    pub fn render_load_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        self.fill_load_form(window, cx);
        let page = crate::screens::overlays::settings_page().track_focus(&self.focus).key_context(LOAD);
        let header = h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER)).child(self.settings_title(cx));
        let body = v().id("load-body").flex_1().min_h_0().overflow_y_scrollbar().p(px(22.)).gap(px(12.));
        let online = matches!(self.connection, Connection::Online { .. });
        let Some(snapshot) = self.server_load().cloned() else {
            let reason = if !online {
                "Not connected to a server.".to_string()
            } else if let Some(error) = self.load_error.clone() {
                error
            } else {
                "Loading…".into()
            };
            let notice = h().gap(px(10.)).px(px(12.)).py(px(10.)).rounded(px(8.)).border_1().border_color(c(theme::ATTENTION_BORDER)).bg(c(theme::ATTENTION_BG))
                .text_size(px(12.5)).text_color(c(theme::ACCENT_TEXT)).child(div().flex_1().child(reason));
            return page.child(header).child(body.child(notice));
        };
        let may_set = self.may_set_load();
        let row = |label: &'static str, value: AnyElement| {
            h().gap(px(12.)).items_center().text_size(px(13.)).child(div().w(px(150.)).flex_none().text_color(c(theme::MUTED)).child(label)).child(div().flex_1().min_w_0().child(value))
        };
        let bar = |fraction: f32, warn: bool| {
            div().w(px(220.)).h(px(6.)).rounded_full().bg(c(theme::SURFACE_2)).child(
                div().h_full().w(relative(fraction.clamp(0., 1.))).rounded_full().bg(c(if warn { theme::NEGATIVE } else { theme::ACCENT })),
            )
        };
        let meter = |fraction: Option<f32>, warn: bool, value: String| {
            h().gap(px(12.))
                .when_some(fraction, |el, f| el.child(bar(f, warn)))
                .child(div().text_color(c(if warn { theme::NEGATIVE } else { theme::TEXT })).child(value))
                .into_any_element()
        };
        let checks = snapshot.checks_active && snapshot.limits.enabled;
        let cpu_warn = checks && snapshot.cpu_pct >= snapshot.limits.cpu_high as f32;
        let mem_warn = checks && snapshot.mem_free_mb < snapshot.limits.mem_min_mb;
        let used = if snapshot.mem_total_mb > 0 { 1. - snapshot.mem_free_mb as f32 / snapshot.mem_total_mb as f32 } else { 0. };
        let (cpu, memory) = if snapshot.checks_active {
            (
                meter(Some(snapshot.cpu_pct / 100.), cpu_warn, format!("{}%", snapshot.cpu_pct.round() as i64)),
                meter(Some(used), mem_warn, format!("{} free of {}", gigabytes(snapshot.mem_free_mb), gigabytes(snapshot.mem_total_mb))),
            )
        } else {
            (meter(None, false, "Not measured".into()), meter(None, false, "Not measured".into()))
        };
        let gate = match waiting_text(&snapshot) {
            Some(text) => h().gap(px(8.)).text_color(c(theme::ACCENT_TEXT)).child(ui::pulse_dot("load-gate", 7., theme::ACCENT)).child(text).into_any_element(),
            None => h().gap(px(8.)).child(ui::dot(7., theme::ACCENT)).child("New runs start right away").into_any_element(),
        };
        let field = |state: &Entity<InputState>, unit: &'static str, explain: &'static str| {
            h().gap(px(10.))
                .child(div().w(px(90.)).flex_none().child(ui::input(state)))
                .child(div().w(px(24.)).flex_none().text_color(c(theme::TEXT_2)).child(unit))
                .child(ui::info(SharedString::from(format!("load-about-{explain}")), explain))
                .into_any_element()
        };
        let form = &self.load_form;
        let enabled = form.enabled;
        let checks_toggle = h().gap(px(10.))
            .child(ui::toggle("load-enabled", enabled, "CPU and memory checks", || Box::new(LoadToggleEnabled)))
            .child(div().flex_1().child(if enabled { "On" } else { "Off" }))
            .child(ui::info("load-checks-about", "On: runs also wait for CPU and memory. Off: only the run limit counts."))
            .child(ui::kbd("alt-e", cx))
            .into_any_element();
        let body = body
            .child(ui::eyebrow("NOW").pt(px(4.)))
            .child(row("CPU", cpu))
            .child(row("Free memory", memory))
            .child(row("Runs", div().child(format!("{} running · {} waiting", snapshot.running, snapshot.held)).into_any_element()))
            .child(row("New runs", gate))
            .when(!snapshot.checks_active, |el| {
                el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child("Checks only active on the standard data folder"))
            })
            .child(ui::eyebrow("LIMITS").pt(px(10.)))
            .child(row("Max runs at once", field(&form.max_runs, "", "0 holds every new run")))
            .child(row("Hold from CPU", field(&form.cpu_high, "%", "New runs wait from this CPU use…")))
            .child(row("Release below CPU", field(&form.cpu_low, "%", "…until it is below this again")))
            .child(row("Min free memory", field(&form.mem_min, "GB", "New runs wait while less memory is free")))
            .child(row("CPU and memory", checks_toggle))
            .when_some(form.error.clone(), |el, e| el.child(div().text_size(px(12.5)).text_color(c(theme::NEGATIVE)).child(e)))
            .when(!may_set, |el| el.child(div().text_size(px(12.5)).text_color(c(theme::NEGATIVE)).child(NO_RIGHT)))
            .child(
                h().gap(px(10.)).pt(px(4.))
                    .child(
                        ui::button("load-save", if form.saving { "Saving…" } else { "Save limits" }, ButtonKind::Primary, Some(ui::kbd("secondary-s", cx)))
                            .when(!may_set, |el| el.opacity(0.45))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save_load_limits(cx))),
                    )
                    .child(ui::info("load-save-about", "Applied at once and kept over restarts.")),
            );
        page.child(header).child(body).child(
            h().px(px(22.)).py(px(12.)).gap(px(14.)).border_t_1().border_color(c(theme::BORDER)).text_size(px(11.5)).text_color(c(theme::MUTED))
                .child(ui::hint("l-edit", "enter", "Edit limits", || Box::new(LoadNextField), cx))
                .child(ui::key_hint(&["tab"], "next field", cx))
                .child(ui::hint("l-save", "secondary-s", "Save", || Box::new(LoadSave), cx))
                .child(ui::hint("l-checks", "alt-e", "Checks on/off", || Box::new(LoadToggleEnabled), cx)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{awaiting_start, gb_field, gigabytes, parse_limits, update_of, waiting_text};
    use aurelius_domain::conversation::{MessageRole, MessageStatus};
    use aurelius_protocol::{LoadGate, LoadHold, LoadHoldKind, LoadLimits, LoadSnapshot, MessageView};

    fn snapshot(kind: Option<LoadHoldKind>) -> LoadSnapshot {
        LoadSnapshot {
            cpu_pct: 82.4,
            mem_free_mb: 1434,
            mem_total_mb: 16384,
            running: 3,
            held: 1,
            gate: if kind.is_some() { LoadGate::Held } else { LoadGate::Open },
            hold: kind.map(|kind| LoadHold { kind, cpu_pct: None, mem_free_mb: None, running: None }),
            limits: LoadLimits { max_runs: 3, cpu_high: 75, cpu_low: 65, mem_min_mb: 2048, enabled: true },
            checks_active: true,
            sampled_at_ms: 1,
        }
    }

    #[test]
    fn the_banner_says_why_runs_wait() {
        assert_eq!(waiting_text(&snapshot(Some(LoadHoldKind::Cpu))).as_deref(), Some("Server busy: CPU 82%, waiting"));
        assert_eq!(waiting_text(&snapshot(Some(LoadHoldKind::Memory))).as_deref(), Some("Server busy: 1.4 GB free, waiting"));
        assert_eq!(waiting_text(&snapshot(Some(LoadHoldKind::MaxRuns))).as_deref(), Some("Server busy: 3 runs active, waiting"));
        assert_eq!(waiting_text(&LoadSnapshot { running: 1, ..snapshot(Some(LoadHoldKind::MaxRuns)) }).as_deref(), Some("Server busy: 1 run active, waiting"));
        assert_eq!(waiting_text(&snapshot(Some(LoadHoldKind::DisabledAll))).as_deref(), Some("Runs paused on the server"));
        assert_eq!(waiting_text(&snapshot(None)), None);
        assert_eq!(gigabytes(20480), "20 GB");
    }

    #[test]
    fn limits_are_checked_before_saving() {
        let ok = parse_limits("3", "80", "60", "1.5", true).unwrap();
        assert_eq!(ok, LoadLimits { max_runs: 3, cpu_high: 80, cpu_low: 60, mem_min_mb: 1536, enabled: true });
        assert_eq!(update_of(ok).cpu_high, Some(80));
        assert!(parse_limits("0", "100", "99", "0", false).is_ok(), "0 runs holds everything; 100 % is allowed");
        assert_eq!(parse_limits("3", "60", "60", "2", true).unwrap_err(), "CPU low must be below CPU high.");
        assert_eq!(parse_limits("3", "60", "70", "2", true).unwrap_err(), "CPU low must be below CPU high.");
        assert!(parse_limits("3", "101", "60", "2", true).unwrap_err().starts_with("CPU high"));
        assert!(parse_limits("-1", "80", "60", "2", true).unwrap_err().starts_with("Max runs"));
        assert!(parse_limits("3", "80", "60", "lots", true).unwrap_err().starts_with("Min free memory"));
        assert_eq!(gb_field(2048), "2");
        assert_eq!(gb_field(1536), "1.5");
    }

    #[test]
    fn only_a_conversation_whose_answer_has_not_started_waits() {
        let message = |role: MessageRole, status: MessageStatus, update: Option<&str>| {
            let mut m: MessageView = serde_json::from_value(serde_json::json!({
                "id": aurelius_domain::MessageId::new(), "ordinal": 0, "role": "user", "status": "complete",
                "text": "", "created_at_ms": 0, "run_id": null, "reply_to": null, "produced_by": null, "author": null, "sources": [], "activity": [], "update": null, "approval_id": null, "files": [], "uploads": [], "queued": false,
            }))
            .unwrap();
            m.role = role;
            m.status = status;
            m.update = update.map(str::to_owned);
            m
        };
        let user = message(MessageRole::User, MessageStatus::Complete, None);
        let answer = message(MessageRole::Assistant, MessageStatus::Complete, None);
        let writing = message(MessageRole::Assistant, MessageStatus::Partial, None);
        let brief = message(MessageRole::Assistant, MessageStatus::Complete, Some("brief"));
        assert!(awaiting_start(&[answer.clone(), user.clone()]));
        assert!(!awaiting_start(&[user.clone(), answer.clone()]));
        assert!(!awaiting_start(&[user.clone(), writing]));
        assert!(awaiting_start(&[brief]));
        assert!(awaiting_start(&[message(MessageRole::Assistant, MessageStatus::Complete, Some("message"))]));
        assert!(!awaiting_start(&[message(MessageRole::Assistant, MessageStatus::Complete, Some("status"))]));
        assert!(!awaiting_start(&[]));
    }
}
