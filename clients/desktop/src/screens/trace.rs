//! Tokens per answer and per conversation or task (feature `run_usage`), and the trace of
//! one run in the side panel (feature `run_trace`): what it sent and what each model call cost.

use crate::app::AppView;
use crate::backend;
use crate::keymap::{LOAD, SettingsTrace};
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_protocol::{RunTrace, TokenUsage, TraceStep, UsageTotals};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Settings → Advanced → Trace.
pub const TRACE_TAB: usize = 12;

/// Cost, "≈" when it is not a bill (subscription: API-equivalent value); empty when not reported.
fn cost(u: &TokenUsage) -> String {
    match u.cost_micro_usd {
        Some(m) => format!("{}{}", if u.estimate { "≈" } else { "" }, ui::fmt_cost(m)),
        None => String::new(),
    }
}

/// Every token the runs sent or got: fresh input (with cache writes), cache reads and output.
fn all_tokens(u: &TokenUsage) -> u64 {
    u.input_tokens + u.cached_tokens + u.output_tokens
}

/// "12.3k in · 1.2k out · 45k cache read · 8k cache write · $0.04". `input_tokens` includes the
/// cache writes, so "in" is the rest: input neither read from nor written to the cache.
pub fn usage_line(u: &TokenUsage) -> String {
    let mut parts = vec![
        format!("{} in", ui::fmt_tokens(u.input_tokens.saturating_sub(u.cache_creation_tokens))),
        format!("{} out", ui::fmt_tokens(u.output_tokens)),
        format!("{} cache read", ui::fmt_tokens(u.cached_tokens)),
        format!("{} cache write", ui::fmt_tokens(u.cache_creation_tokens)),
    ];
    parts.extend(Some(cost(u)).filter(|c| !c.is_empty()));
    parts.join(" · ")
}

fn usage_detail(u: &TokenUsage) -> String {
    let fresh = u.input_tokens.saturating_sub(u.cache_creation_tokens);
    let mut lines = vec![
        format!("Input, not cached: {fresh}"),
        format!("Cache write: {} (dearer than input, cheap to read later)", u.cache_creation_tokens),
        format!("Cache read: {} (about a tenth of input)", u.cached_tokens),
        format!("Output: {}", u.output_tokens),
        format!("Total: {} tokens over {} run{}", all_tokens(u), u.runs, if u.runs == 1 { "" } else { "s" }),
    ];
    lines.push(match u.cost_micro_usd {
        Some(m) if u.estimate => format!("{} API-equivalent value (subscription: no extra bill)", ui::fmt_cost(m)),
        Some(m) => format!("Cost: {}", ui::fmt_cost(m)),
        None => "Cost not reported".into(),
    });
    lines.join("\n")
}

/// How dear a model call is, in input-token units (Anthropic's price ratios:
/// cache write 1.25×, cache read 0.1×, output 5× input).
pub fn step_weight(s: &TraceStep) -> f64 {
    s.input_tokens as f64 + 1.25 * s.cache_creation_tokens as f64 + 0.1 * s.cache_read_tokens as f64 + 5.0 * s.output_tokens as f64
}

impl AppView {
    /// The token line under an answer, with the details in its tooltip.
    pub fn usage_row(&self, key: String, u: &TokenUsage) -> Stateful<Div> {
        div().id(SharedString::from(format!("usage-{key}"))).text_size(px(11.)).text_color(c(theme::MUTED))
            .child(usage_line(u))
            .tooltip(ui::tip(usage_detail(u), None))
    }

    /// The open conversation's (or task's) total for its header, tasks given away apart in the tooltip.
    pub fn usage_total_chip(&self) -> Option<Stateful<Div>> {
        let (_, t): &(_, UsageTotals) = self.usage_totals.as_ref().filter(|(id, _)| self.open_conversation == Some(*id) && self.run_usage_on())?;
        let mut tip = vec![format!("This conversation: {}", usage_line(&t.own))];
        tip.extend(t.subtasks.iter().map(|s| {
            let who = s.employee_name.as_deref().map(|n| format!(" ({n})")).unwrap_or_default();
            format!("{}{who}: {}", s.title, usage_line(&s.usage))
        }));
        if !t.subtasks.is_empty() {
            tip.push(format!("Total: {}", usage_line(&t.total)));
        }
        let label = match cost(&t.total) {
            c if c.is_empty() => format!("{} tokens", ui::fmt_tokens(all_tokens(&t.total))),
            c => format!("{} tokens · {c}", ui::fmt_tokens(all_tokens(&t.total))),
        };
        let label = if t.subtasks.is_empty() { label } else { format!("{label} · {} subtask{}", t.subtasks.len(), if t.subtasks.len() == 1 { "" } else { "s" }) };
        Some(
            div().id("usage-total").flex_none().px(px(8.)).py(px(2.)).rounded(px(5.)).border_1().border_color(c(theme::BORDER))
                .text_size(px(11.5)).text_color(c(theme::TEXT_2))
                .child(label)
                .tooltip(ui::tip(tip.join("\n"), None)),
        )
    }

    pub(crate) fn render_trace_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let body: AnyElement = match self.trace.as_ref().map(|(_, t)| t) {
            Some(Some(Ok(trace))) => self.trace_body(&trace.clone(), cx).into_any_element(),
            Some(Some(Err(why))) => div().text_color(c(theme::MUTED)).child(why.clone()).into_any_element(),
            _ => div().text_color(c(theme::MUTED)).child("Loading…").into_any_element(),
        };
        // The guideline versions the run had (B6, P-59): also when trace mode kept no trace.
        let guidelines = self.trace.as_ref().and_then(|(run, _)| self.render_run_guidelines(*run, cx));
        let panel = self.side_panel("TRACE", cx)
            .child(v().id("trace-body").flex_1().min_h_0().overflow_y_scrollbar().px(px(28.)).py(px(20.)).gap(px(18.)).children(guidelines).child(body))
            .child(
                h().px(px(28.)).py(px(12.)).gap(px(10.)).border_t_1().border_color(c(theme::BORDER))
                    .child(h().flex_1().child(ui::info("trace-estimates-about", "Token counts per part are estimates (4 characters ≈ 1 token); the steps are as the model reported them.")))
                    .child(ui::kbd("escape", cx).opacity(0.6)),
            );
        self.slide_in(panel, ("trace-scrim", "trace-slide"), ("trace_panel", 780.), cx)
    }

    fn trace_body(&self, t: &RunTrace, cx: &mut Context<Self>) -> Div {
        let mono = ui::mono(cx);
        let small = |text: String| div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(text);
        let session = match t.session.as_str() {
            "continue" | "resume" => "resumed session: only the new messages",
            "new" => "new session: the whole conversation sent again",
            _ => "no session",
        };
        let facts = [
            Some(format!("Model {}", t.model)),
            t.effort.as_ref().map(|e| format!("effort {e}")),
            t.mode.as_ref().map(|m| format!("mode {m}")),
            Some(format!("tools {}", t.profile)),
            Some(session.to_string()),
            Some(ui::fmt_when(t.created_at_ms)),
        ];
        let max_part = t.parts.iter().map(|p| p.tokens).max().unwrap_or(0).max(1);
        let parts = v().gap(px(2.)).children(t.parts.iter().map(|p| {
            h().gap(px(12.)).py(px(4.)).text_size(px(12.5)).border_b_1().border_color(c(theme::SURFACE_2))
                .child(div().w(px(60.)).flex_none().text_color(c(theme::MUTED)).child(p.area.clone()))
                .child(div().flex_1().min_w_0().truncate().child(p.label.clone()))
                .child(div().w(px(120.)).flex_none().h(px(6.)).rounded(px(2.)).bg(c(theme::SURFACE_2))
                    .child(div().h_full().rounded(px(2.)).bg(c(theme::BORDER_STRONG)).w(relative(p.tokens as f32 / max_part as f32))))
                .child(div().w(px(150.)).flex_none().font_family(mono.clone()).text_size(px(11.5)).text_color(c(theme::TEXT_2))
                    .child(format!("{} chars · ~{}", ui::fmt_tokens(p.chars), ui::fmt_tokens(p.tokens))))
        }));
        let max_weight = t.steps.iter().map(step_weight).fold(0.0, f64::max).max(1.0);
        let cell = |w: f32, text: String| div().w(px(w)).flex_none().font_family(mono.clone()).text_size(px(11.5)).child(text);
        let steps = v().gap(px(1.))
            .child(h().gap(px(8.)).py(px(4.)).text_size(px(11.)).text_color(c(theme::MUTED))
                .child(div().w(px(26.)).flex_none().child("#"))
                .child(div().w(px(64.)).flex_none().child("in"))
                .child(div().w(px(70.)).flex_none().child("cache write"))
                .child(div().w(px(70.)).flex_none().child("cache read"))
                .child(div().w(px(56.)).flex_none().child("out"))
                .child(div().w(px(90.)).flex_none().child("cost weight"))
                .child(div().flex_1().child("tools asked")))
            .children(t.steps.iter().enumerate().map(|(i, s)| {
                let share = (step_weight(s) / max_weight) as f32;
                // The dearest calls stand out.
                let dear = share >= 0.5 && t.steps.len() > 1;
                let tools = if s.tools.is_empty() { "—".to_string() } else { s.tools.join(", ") };
                h().gap(px(8.)).py(px(3.)).px(px(4.)).rounded(px(4.)).text_color(c(theme::TEXT))
                    .when(dear, |el| el.bg(c(theme::ATTENTION_BG)))
                    .child(cell(26., format!("{}", i + 1)))
                    .child(cell(64., ui::fmt_tokens(s.input_tokens)))
                    .child(cell(70., ui::fmt_tokens(s.cache_creation_tokens)))
                    .child(cell(70., ui::fmt_tokens(s.cache_read_tokens)))
                    .child(cell(56., ui::fmt_tokens(s.output_tokens)))
                    .child(div().w(px(90.)).flex_none().h(px(6.)).rounded(px(2.)).bg(c(theme::SURFACE_2))
                        .child(div().h_full().rounded(px(2.)).bg(c(if dear { theme::ACCENT } else { theme::BORDER_STRONG })).w(relative(share))))
                    .child(div().flex_1().min_w_0().truncate().text_size(px(11.5)).text_color(c(theme::TEXT_2))
                        .child(if s.subagent { format!("subagent · {tools}") } else { tools }))
            }));
        let block = |key: usize, title: &'static str, text: &str, cx: &mut Context<Self>| {
            let open = self.trace_open[key];
            v().gap(px(6.))
                .child(h().id(("trace-fold", key)).role(Role::Button).gap(px(8.)).cursor_pointer()
                    .hover(|s| s.text_color(c(theme::TEXT)))
                    .child(ui::eyebrow(title))
                    .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(format!("{} chars · ~{} tokens", ui::fmt_tokens(text.chars().count() as u64), ui::fmt_tokens(text.len() as u64 / 4))))
                    .child(div().flex_1())
                    .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(if open { "▴ Hide" } else { "▾ Show" }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.trace_open[key] = !this.trace_open[key];
                        cx.notify();
                    })))
                .when(open, |el| el.child(
                    div().p(px(12.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                        .font_family(ui::mono(cx)).text_size(px(11.5)).line_height(px(17.)).text_color(c(theme::TEXT_2))
                        .child(text.to_string()),
                ))
        };
        v().gap(px(18.))
            .child(v().gap(px(6.))
                .child(small(facts.into_iter().flatten().collect::<Vec<_>>().join(" · ")))
                .children(t.usage.as_ref().map(|u| div().text_size(px(13.)).text_color(c(theme::TEXT)).child(usage_line(u))))
                .child(small(format!("MCP servers: {}", if t.mcp_servers.is_empty() { "none".to_string() } else { t.mcp_servers.join(", ") })))
                .child(small(format!("{} tools loaded (their definitions go with every model call)", t.tools.len()))))
            .child(v().gap(px(6.)).child(ui::eyebrow("SIZE PER PART")).child(parts))
            .child(block(0, "SYSTEM TEXT", &t.system_text, cx))
            .child(block(1, "USER TURN", &t.user_turn, cx))
            .when(!t.images.is_empty(), |el| el.child(v().gap(px(6.)).child(ui::eyebrow("FILES AND IMAGES"))
                .children(t.images.iter().map(|i| small(format!("{} · {} · {}", i.name, i.media_type, if i.bytes >= 1_048_576 { format!("{:.1} MB", i.bytes as f64 / 1_048_576.0) } else { format!("{} KB", (i.bytes / 1024).max(1)) }))))))
            .when(!t.args.is_empty(), |el| el.child(v().gap(px(6.)).child(ui::eyebrow("COMMAND LINE"))
                .child(div().p(px(12.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                    .font_family(mono.clone()).text_size(px(11.5)).line_height(px(17.)).text_color(c(theme::TEXT_2)).child(t.args.join(" ")))))
            .child(v().gap(px(6.))
                .child(ui::eyebrow(format!("MODEL CALLS · {}", t.steps.len())))
                .when(t.steps.is_empty(), |el| el.child(small("No model calls recorded (yet).".into())))
                .when(!t.steps.is_empty(), |el| el.child(steps)))
    }
}

#[cfg(test)]
mod tests {
    use super::{TokenUsage, TraceStep, step_weight, usage_line};

    #[test]
    fn line_splits_cache_writes_off_the_input() {
        let u = TokenUsage { runs: 1, input_tokens: 20_300, output_tokens: 1_200, cached_tokens: 45_000, cache_creation_tokens: 8_000, cost_micro_usd: Some(40_000), estimate: false };
        assert_eq!(usage_line(&u), "12.3k in · 1.2k out · 45k cache read · 8k cache write · $0.04");
        assert_eq!(usage_line(&TokenUsage { cost_micro_usd: None, ..u.clone() }), "12.3k in · 1.2k out · 45k cache read · 8k cache write");
    }

    #[test]
    fn output_and_cache_writes_weigh_more_than_cache_reads() {
        let read = TraceStep { cache_read_tokens: 40_000, ..Default::default() };
        let write = TraceStep { cache_creation_tokens: 8_000, ..Default::default() };
        assert!(step_weight(&write) > step_weight(&read));
        assert_eq!(step_weight(&TraceStep { output_tokens: 10, ..Default::default() }), 50.0);
    }
}

impl AppView {
    /// Settings → Advanced → Trace: ask for the setting again.
    pub fn enter_trace_tab(&mut self) {
        if self.trace_supported() {
            let _ = self.requests.send(backend::Request::LoadTraceSetting);
        }
    }

    fn trace_settings<E: ParentElement + gpui_kit::prelude::FluentBuilder>(&self, el: E, cx: &mut Context<Self>) -> E {
        let section = |title: &'static str| div().mt(px(8.)).pt(px(14.)).border_t_1().border_color(c(theme::BORDER)).child(ui::eyebrow(title));
        el.when_some(self.trace_setting.filter(|_| self.trace_supported()), |el, s| {
            let note = format!(
                "Keeps what each answer sends (system text, prompt, files, command line) and the tokens of every model call; Trace under an answer shows it. The latest {} runs, {} days. Turning it off deletes the kept traces. Applies from the next run.",
                s.keep_runs, s.keep_days
            );
            el.child(section("TRACE MODE")).child(
                h().gap(px(12.)).items_center().text_size(px(13.))
                    .child(div().w(px(130.)).flex_none().text_color(c(theme::MUTED)).child(ui::label_with_info("Trace mode", note)))
                    .child(ui::toggle("trace-enabled", s.enabled, "Trace mode", || Box::new(SettingsTrace))),
            )
            .when(self.trace_confirm_off, |el| el.child(
                h().gap(px(12.)).items_center().text_size(px(12.5))
                    .child(div().w(px(130.)).flex_none())
                    .child(div().flex_1().text_color(c(theme::NEGATIVE)).child("Turn trace mode off? Every kept trace is deleted."))
                    .child(ui::button("trace-off-yes", "Turn off and delete", ButtonKind::Primary, None).h(px(28.))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.set_trace_enabled(false, cx))))
                    .child(ui::button("trace-off-no", "Keep on", ButtonKind::Outline, None).h(px(28.))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.trace_confirm_off = false;
                            cx.notify();
                        }))),
            ))
        })
    }

    /// Settings → Advanced → Trace: the page.
    pub fn render_trace_settings(&mut self, page: Div, cx: &mut Context<Self>) -> Div {
        let supported = self.trace_supported();
        let body = v().id("trace-body").flex_1().min_h_0().overflow_y_scrollbar().p(px(22.)).gap(px(12.));
        let body = if supported { self.trace_settings(body, cx) } else { body.child(div().text_size(px(13.)).text_color(c(theme::MUTED)).child("This server does not have trace mode. Update the server to use it.")) };
        // LOAD context: Tab and Esc only, no single-letter shortcuts on this page.
        page.key_context(LOAD)
            .child(h().px(px(22.)).py(px(14.)).border_b_1().border_color(c(theme::BORDER)).child(self.settings_title(cx)))
            .child(body)
    }
}
