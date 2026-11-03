//! The library and the editor (with History) of Employees › Guidelines.

use super::logic::*;
use super::state::*;
use crate::app::AppView;
use crate::backend::{self, GuidelineCall, GuidelineWrite};
use crate::screens::org::line_diff;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::GuidelineProposalId;
use aurelius_domain::guideline::GuidelineView;
use gpui_kit::component::input::{Textarea, TextareaState};
use gpui_kit::base::TestSupportExt as _;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::BTreeSet;

/// A box: ticked, half (a group with some), empty; faded when it can't change.
pub(super) fn tick(state: Tri, enabled: bool) -> Div {
    div()
        .size(px(15.))
        .flex_none()
        .rounded(px(3.))
        .border_1()
        .border_color(c(if state == Tri::None { theme::BORDER_STRONG } else { theme::ACCENT }))
        .when(state != Tri::None, |el| el.bg(c(theme::ACCENT)))
        .when(!enabled, |el| el.opacity(0.4))
        .flex()
        .items_center()
        .justify_center()
        .when(state == Tri::All, |el| el.child(ui::check(9., theme::BG)))
        .when(state == Tri::Some, |el| el.child(div().w(px(7.)).h(px(2.)).bg(c(theme::BG))))
}

pub(super) fn on(b: bool) -> Tri {
    if b { Tri::All } else { Tri::None }
}

/// A bar of `chars` against `limit`, coloured by its level.
pub(super) fn bar(chars: u32, limit: u32, width: f32) -> Div {
    let share = (chars as f32 / limit.max(1) as f32).min(1.);
    div().w(px(width)).flex_none().h(px(6.)).rounded(px(3.)).bg(c(theme::SURFACE_2))
        .child(div().h_full().rounded(px(3.)).bg(c(level_color(level(chars, limit)))).w(relative(share)))
}

/// A colour for a figure at `level`: the usual grey while calm.
pub(super) fn level_text(level: Level) -> u32 {
    if level == Level::Calm { theme::TEXT_2 } else { level_color(level) }
}

/// The lines of a diff: `+` and `-` as a sign and a colour, monospace (§4.2, §9). Plain
/// text: nothing in it is markdown or loads (E14).
pub fn diff_box(old: &str, new: &str, mono: SharedString) -> Div {
    v().p(px(10.)).rounded(px(6.)).bg(c(theme::SURFACE)).border_1().border_color(c(theme::BORDER)).font_family(mono).text_size(px(12.)).line_height(px(17.))
        .children(line_diff(old, new).into_iter().map(|(mark, line)| {
            let color = match mark {
                '+' => theme::ACCENT_TEXT,
                '-' => theme::NEGATIVE,
                _ => theme::TEXT_2,
            };
            div().text_color(c(color)).when(mark != ' ', |el| el.bg(c(theme::SURFACE_2))).child(format!("{mark} {line}"))
        }))
}

/// The editor's text box: as tall as the preview beside it (Pulse M1).
pub(super) fn write_pane(text: &Entity<TextareaState>, height: f32, mono: SharedString) -> impl IntoElement {
    div().id("gl-write").test_support().flex_1().min_w_0().child(Textarea::new(text).h(px(height)).py(px(7.)).line_height(px(20.)).font_family(mono).text_size(px(13.)))
}

/// The preview pane, `height` tall and scrolling within.
pub(super) fn preview_pane(height: f32, content: impl IntoElement) -> impl IntoElement {
    div().id("gl-preview").test_support().flex_1().min_w_0().h(px(height)).rounded(px(6.)).border_1().border_color(c(theme::BORDER))
        .child(v().id("gl-preview-text").size_full().overflow_y_scrollbar().px(px(14.)).py(px(8.)).child(content))
}

/// Write, Preview or both side by side.
pub(super) fn panes(mode: Mode, write: impl IntoElement, preview: impl IntoElement) -> Div {
    let row = h().gap(px(12.)).items_start();
    match mode {
        Mode::Write => row.child(write),
        Mode::Preview => row.child(preview),
        Mode::Side => row.child(write).child(preview),
    }
}

impl AppView {
    /// A preview of markdown as the team reads it, without HTML or anything that loads (E14).
    fn guideline_preview(&self, key: &str, text: &str, cx: &mut Context<Self>) -> AnyElement {
        use std::hash::{Hash, Hasher};
        let safe = safe_markdown(text);
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        safe.hash(&mut hasher);
        // Base's view: the component wrapper has no image resolver, and adds nothing here.
        let view = gpui_kit::base::TextView::markdown(SharedString::from(format!("gl-{key}-{:x}", hasher.finish())), safe)
            .selectable(true)
            // Fail-closed beside safe_markdown: no image ever loads, whatever the parser makes of the text.
            .image_source(no_image)
            .on_link_click(self.markdown_link_handler(false, cx));
        div().text_size(px(14.)).line_height(px(22.)).child(view).into_any_element()
    }

    /// Employees › Guidelines.
    pub fn render_guidelines_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Some((title, text)) = self.guidelines.fill.take() {
            self.guidelines.title.update(cx, |i, cx| i.set_value(title, window, cx));
            self.guidelines.text.update(cx, |i, cx| i.set_value(text, window, cx));
        }
        if !self.guidelines_ready() {
            let why = match &self.connection {
                backend::Connection::Online { .. } => backend::GUIDELINES_NOT_BUILT,
                _ => "Not connected to a server.",
            };
            return v().p(px(40.)).child(div().text_color(c(theme::MUTED)).child(why)).into_any_element();
        }
        self.guidelines_want();
        match self.guidelines.view {
            GuidelinesView::Editor if self.guidelines.editor.is_some() => self.render_guideline_editor(window, cx),
            GuidelinesView::Employee(id) => self.render_guidelines_of(id, cx),
            _ => self.render_guideline_library(cx),
        }
    }

    fn render_guideline_library(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let can_edit = self.guidelines_can_edit();
        let archived = self.guidelines.archived;
        let names = self.gl_names();
        let source = if archived { &self.guidelines.archived_list } else { &self.guidelines.list };
        let query = self.guidelines.query.read(cx).value().to_string();
        let me = self.active_member().map(|u| u.id.to_string());
        let banner = !archived && can_edit && banner_due(self.guidelines.active(), me.as_deref(), &self.guidelines.banner_done);
        let (rows, empty, error): (Vec<GuidelineView>, bool, Option<String>) = match source {
            None => (Vec::new(), false, None),
            Some(Err(e)) => (Vec::new(), false, Some(e.clone())),
            Some(Ok(list)) => (shown(list, &query, self.guidelines.sort, names).into_iter().cloned().collect(), list.is_empty(), None),
        };
        let loading = source.is_none();
        let total = match source {
            Some(Ok(list)) => list.len(),
            _ => 0,
        };
        let head = h().gap(px(10.)).items_center()
            .child(div().font_family(ui::serif(cx)).text_size(px(30.)).child("Guidelines"))
            .child(ui::info("guidelines-about", ABOUT))
            .child(div().flex_1())
            .child(div().w(px(240.)).child(ui::input(&self.guidelines.query)))
            .child(
                h().id("gl-archived").role(Role::Button).aria_label("Archived").gap(px(6.)).px(px(10.)).h(px(32.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).cursor_pointer().text_size(px(12.5))
                    .when(archived, |el| el.bg(c(theme::SURFACE_2)))
                    .hover(|s| s.bg(c(theme::SURFACE_2)))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.guidelines.archived = !this.guidelines.archived;
                        cx.notify();
                    }))
                    .child(tick(on(archived), true))
                    .child("Archived"),
            )
            .when(can_edit, |el| {
                el.child(ui::button("gl-new", "+ New guideline", ButtonKind::Primary, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.guideline_open(None, w, cx))))
            });
        let sub = div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(if archived {
            format!("{total} archived · no run gets these; Restore puts one back")
        } else {
            format!("{total} guideline{} · rules every linked run gets, on top of its instructions", if total == 1 { "" } else { "s" })
        });
        let mut body = v().gap(px(12.)).child(head).child(sub);
        if banner {
            body = body.child(
                h().gap(px(12.)).px(px(14.)).py(px(10.)).rounded(px(8.)).border_1().border_color(c(theme::ATTENTION_BORDER)).bg(c(theme::ATTENTION_BG))
                    .child(div().flex_1().min_w_0().text_size(px(13.)).line_height(px(19.)).child(BANNER))
                    .child(ui::button("gl-banner", "Got it", ButtonKind::Outline, None).flex_none().on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.guideline_banner_done(cx)))),
            );
        }
        body = if let Some(e) = error {
            body.child(div().text_size(px(13.)).text_color(c(theme::NEGATIVE)).child(e))
        } else if loading {
            body.child(div().text_size(px(13.)).text_color(c(theme::MUTED)).child("Loading…"))
        } else if empty && !archived {
            body.child(self.render_guidelines_empty(cx))
        } else if rows.is_empty() {
            body.child(div().text_size(px(13.)).text_color(c(theme::MUTED)).child(if archived { "Nothing is archived." } else { "No guideline matches." }))
        } else {
            body.child(self.render_guideline_rows(&rows, archived, cx))
        };
        if !archived && !empty {
            body = body.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(format!("◆n = proposals wait for you · Size in characters (max {} each)", count(TEXT_LIMIT))));
        }
        self.page(v().pt(px(32.)).pb(px(40.)).px(px(28.)).child(body)).into_any_element()
    }

    fn render_guidelines_empty(&self, cx: &mut Context<Self>) -> Div {
        let can_edit = self.guidelines_can_edit();
        v().items_center().gap(px(10.)).py(px(40.)).px(px(40.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER))
            .child(div().font_family(ui::serif(cx)).text_size(px(22.)).child("No guidelines yet"))
            .child(div().max_w(px(520.)).text_size(px(13.)).line_height(px(19.)).text_color(c(theme::TEXT_2)).text_center().child(EMPTY_ABOUT))
            .when(can_edit, |el| {
                el.child(
                    h().gap(px(10.)).pt(px(6.))
                        .child(ui::button("gl-empty-new", "+ New guideline", ButtonKind::Primary, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.guideline_open(None, w, cx))))
                        .child(ui::button("gl-empty-starter", "Add starter set…", ButtonKind::Outline, None).on_click(cx.listener(|this, _: &ClickEvent, w, cx| {
                            this.guideline_send(GuidelineCall::StarterSet);
                            this.guideline_dialog(Dialog::Starter { templates: None, picked: BTreeSet::new(), busy: false }, w, cx);
                        }))),
                )
            })
            .when(!can_edit, |el| el.child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(READ_ONLY)))
            .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child(EMPTY_PROPOSE))
    }

    fn render_guideline_rows(&self, rows: &[GuidelineView], archived: bool, cx: &mut Context<Self>) -> Div {
        let names = self.gl_names();
        let can_edit = self.guidelines_can_edit();
        let sort = self.guidelines.sort;
        let header = |id: &'static str, label: &'static str, w: Option<f32>, s: Option<Sort>, cx: &mut Context<Self>| {
            let active = s.is_some() && s == Some(sort);
            let mut cell = h().id(id).gap(px(4.)).text_size(px(11.5)).text_color(c(if active { theme::TEXT } else { theme::MUTED }));
            cell = match w {
                Some(w) => cell.w(px(w)).flex_none(),
                None => cell.flex_1().min_w_0(),
            };
            if let Some(s) = s {
                cell = cell.cursor_pointer().hover(|st| st.text_color(c(theme::TEXT))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.guidelines.sort = s;
                    cx.notify();
                }));
            }
            cell.child(label).when(active, |el| el.child("▾"))
        };
        let head = h().gap(px(12.)).px(px(10.)).py(px(6.)).border_b_1().border_color(c(theme::BORDER))
            .child(header("gl-h-title", "Title", Some(200.), Some(Sort::Title), cx))
            .child(header("gl-h-applies", "Applies to", None, None, cx))
            .child(header("gl-h-version", "Version", Some(70.), None, cx))
            .child(header("gl-h-size", "Size", Some(70.), Some(Sort::Size), cx))
            .child(header("gl-h-updated", "Updated", Some(150.), Some(Sort::Updated), cx))
            .when(archived && can_edit, |el| el.child(div().w(px(90.)).flex_none()));
        let me = self.active_member().map(|u| u.id);
        let pending = self.guidelines.pending();
        let rows: Vec<Stateful<Div>> = rows.iter().enumerate().map(|(i, g)| {
            let id = g.id;
            let (applies, all) = applies_to(&g.links, names);
            let who = match &g.updated_by {
                Some(u) if Some(u.id) == me => "You".to_string(),
                Some(u) => u.username.clone(),
                None => "Starter set".to_string(),
            };
            let open = g.open_proposals.max(pending.iter().filter(|p| p.guideline_id == Some(id)).count() as u32);
            let first = pending.iter().find(|p| p.guideline_id == Some(id)).map(|p| p.id);
            h().id(("gl-row", i)).gap(px(12.)).px(px(10.)).py(px(8.)).rounded(px(6.)).cursor_pointer().text_size(px(13.))
                .hover(|s| s.bg(c(theme::SURFACE_2)))
                .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_open(Some(id), w, cx)))
                .child(div().w(px(200.)).flex_none().truncate().font_weight(FontWeight::MEDIUM).child(g.title.clone()))
                .child(div().id(("gl-applies", i)).flex_1().min_w_0().truncate().text_color(c(if g.links.is_empty() { theme::TEXT_2 } else { theme::TEXT })).tooltip(ui::tip(all, None)).child(applies))
                .child(
                    h().w(px(70.)).flex_none().gap(px(6.)).child(format!("v{}", g.version))
                        .when(open > 0, |el| {
                            el.child(
                                h().id(("gl-open", i)).gap(px(3.)).text_color(c(theme::ACCENT_TEXT)).cursor_pointer()
                                    .tooltip(ui::tip(if open == 1 { "1 proposal waits for you".to_string() } else { format!("{open} proposals wait for you") }, None))
                                    .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                                        cx.stop_propagation();
                                        this.guideline_show_proposal(first, w, cx);
                                    }))
                                    .child(ui::diamond(7., theme::ACCENT_TEXT, false))
                                    .child(open.to_string()),
                            )
                        }),
                )
                .child(div().w(px(70.)).flex_none().text_color(c(theme::TEXT_2)).child(count(g.chars)))
                .child(div().w(px(150.)).flex_none().truncate().text_size(px(12.)).text_color(c(theme::MUTED)).child(format!("{who} · {}", ui::fmt_day(g.updated_at_ms))))
                .when(archived && can_edit, |el| {
                    el.child(ui::button(("gl-restore", i), "Restore", ButtonKind::Outline, None).w(px(90.)).h(px(26.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.guideline_send(GuidelineCall::Write(GuidelineWrite::Restore(id)));
                    })))
                })
        }).collect();
        v().child(head).children(rows)
    }

    /// ◆n and "Review ›": the dialog of this proposal, or of the first that waits.
    pub(crate) fn guideline_show_proposal(&mut self, id: Option<GuidelineProposalId>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = id.or_else(|| self.pending_guideline_proposals().first().map(|p| p.id)) else { return };
        self.open_decision(crate::app::Decision::Guideline(id), window, cx);
    }

    fn render_guideline_editor(&mut self, window: &Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(e) = self.guidelines.editor.clone() else { return div().into_any_element() };
        let names = self.gl_names();
        let can_edit = self.guidelines_can_edit() && !e.archived;
        let (title, text) = self.guideline_typed(cx);
        let n = chars(&text);
        let largest = self.guideline_largest(n);
        let current = e.guideline.and_then(|id| self.guidelines.guideline(id)).cloned();
        let heading = match (&current, e.proposal) {
            (_, Some(_)) => "Edit and approve".to_string(),
            (Some(g), None) => g.title.clone(),
            (None, None) => "New guideline".to_string(),
        };
        let mono = ui::mono(cx);
        let (applies, all) = applies_to(&self.guideline_editor_links(), names);
        let subject = match (e.guideline, e.proposal) {
            (Some(id), _) => Subject::Guideline(id),
            (None, Some(p)) => Subject::Proposal(p),
            (None, None) => Subject::Draft,
        };
        // A proposal of an agent for this one: a bar on top (§7.1).
        let waiting = e.guideline.and_then(|id| self.guidelines.pending().into_iter().find(|p| p.guideline_id == Some(id)).map(|p| (p.id, names.employee(p.proposed_by))));
        let back = h().id("gl-back").gap(px(6.)).cursor_pointer().text_size(px(12.5)).text_color(c(theme::MUTED)).hover(|s| s.text_color(c(theme::TEXT)))
            .on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.guideline_leave(GuidelinesView::Library, w, cx)))
            .child(format!("‹ Guidelines · {heading}"));
        let mode_tab = |id: &'static str, label: &'static str, mode: Mode, cx: &mut Context<Self>| {
            let active = e.mode == mode;
            h().id(id).role(Role::Tab).aria_label(label).px(px(12.)).h(px(28.)).rounded(px(6.)).cursor_pointer().text_size(px(12.5))
                .text_color(c(if active { theme::TEXT } else { theme::MUTED }))
                .when(active, |el| el.bg(c(theme::SURFACE_2)).font_weight(FontWeight::MEDIUM))
                .hover(|s| s.text_color(c(theme::TEXT)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if let Some(e) = this.guidelines.editor.as_mut() {
                        e.mode = mode;
                    }
                    cx.notify();
                }))
                .child(label)
        };
        let top = h().gap(px(12.)).items_center()
            .child(div().w(px(60.)).flex_none().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child("Title *"))
            .child(div().w(px(320.)).child(ui::input(&self.guidelines.title)))
            .child(div().flex_1())
            .child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Applies to"))
            .child(div().id("gl-editor-applies").max_w(px(320.)).truncate().text_size(px(12.5)).tooltip(ui::tip(all, None)).child(applies))
            .when(can_edit, |el| {
                el.child(ui::button("gl-links", "Links…", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_links(subject, w, cx))))
            });
        let archive_id = e.guideline.filter(|_| can_edit && e.proposal.is_none() && current.as_ref().is_some_and(|g| !g.archived));
        let tabs = h().gap(px(4.)).items_center()
            .child(mode_tab("gl-mode-write", "Write", Mode::Write, cx))
            .child(mode_tab("gl-mode-preview", "Preview", Mode::Preview, cx))
            .child(mode_tab("gl-mode-side", "Side by side", Mode::Side, cx))
            .child(div().flex_1())
            .when(e.guideline.is_some(), |el| {
                el.child(ui::button("gl-history", if e.history { "History ◂" } else { "History ▸" }, ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    let Some(e) = this.guidelines.editor.as_mut() else { return };
                    e.history = !e.history;
                    if let (true, Some(id)) = (e.history, e.guideline) {
                        this.guideline_send(GuidelineCall::Versions(id));
                    }
                    cx.notify();
                })))
            })
            .when_some(archive_id, |el, id| {
                el.child(ui::button("gl-archive", "Archive…", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_dialog(Dialog::Confirm(Confirm::Archive(id)), w, cx))))
            });
        let height = pane_height(f32::from(window.viewport_size().height));
        let write = write_pane(&self.guidelines.text, height, mono.clone());
        let preview = preview_pane(height, self.guideline_preview("editor", &text, cx));
        let panes = panes(e.mode, write, preview);
        let text_level = level(n, TEXT_LIMIT);
        let block_level = largest.map_or(Level::Calm, |b| level(b.chars, RUN_LIMIT));
        let counters = h().gap(px(24.)).items_start()
            .child(
                v().gap(px(4.)).flex_1()
                    .child(
                        h().gap(px(10.)).text_size(px(12.5))
                            .child(div().id("gl-counter").aria_label(format!("{} of {} characters", count(n), count(TEXT_LIMIT))).child(counter_line(n)))
                            .children(counter_note(n).map(|t| div().text_color(c(level_color(text_level))).child(t))),
                    )
                    .child(bar(n, TEXT_LIMIT, 240.)),
            )
            .child(
                v().gap(px(4.)).flex_1().text_size(px(12.5))
                    .child(h().gap(px(6.)).child(div().text_color(c(theme::MUTED)).child("Largest block with this:")).child(largest.map_or_else(|| "no run gets it yet".to_string(), |b| block_where(b, names))))
                    .children(largest.map(|b| {
                        h().gap(px(10.)).items_center()
                            .child(bar(b.chars, RUN_LIMIT, 160.))
                            .child(div().text_color(c(level_text(block_level))).child(format!("{} / {} · {} per run", count(b.chars), count(RUN_LIMIT), tokens(b.chars))))
                    })),
            );
        let problem = save_problem(&title, &text, largest, names);
        let failure = e.failure.as_ref().map(|f| failure_text(f, names));
        let affected = e.guideline.and_then(|id| self.guidelines.affected.get(&id)).and_then(|a| a.as_ref().ok()).map(affected_line);
        let keeps = match (e.guideline, e.base_version) {
            (Some(_), v) if v > 0 => format!(" Runs that already started keep version {v}."),
            _ => String::new(),
        };
        let applies_line = format!("{}{keeps}", affected.unwrap_or_else(|| "Applies from the next run.".into()));
        let save_label = if e.proposal.is_some() { "Approve" } else { "Save" };
        let off = problem.is_some() || e.busy || !e.loaded;
        let footer = h().gap(px(10.)).items_center()
            // min_w_0: the line wraps, so Cancel and Save stay in view beside a task's panel (Pulse m3).
            .child(div().flex_1().min_w_0().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(applies_line))
            .child(ui::button("gl-cancel", "Cancel", ButtonKind::Outline, None).flex_none().on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.guideline_leave(GuidelinesView::Library, w, cx))))
            .when(can_edit, |el| {
                el.child(
                    ui::button("gl-save", if e.busy { "Saving…" } else { save_label }, ButtonKind::Primary, Some(ui::kbd("secondary-enter", cx)))
                        .flex_none()
                        .when(off, |el| el.opacity(0.5))
                        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.guideline_save(cx))),
                )
            });
        let conflict = e.conflict.map(|newer| {
            let compare = e.compare;
            let theirs = e.guideline.and_then(|id| self.guidelines.texts.get(&(id, newer))).map(|v| v.text.clone());
            v().gap(px(8.)).px(px(12.)).py(px(10.)).rounded(px(7.)).border_1().border_color(c(theme::NEGATIVE))
                .child(div().text_size(px(12.5)).text_color(c(theme::NEGATIVE)).child(format!("Version {newer} was saved while you were editing. Nothing of yours was saved.")))
                .child(
                    h().gap(px(8.))
                        .child(ui::button("gl-compare", if compare { "Hide compare" } else { "Compare" }, ButtonKind::Outline, None).h(px(26.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            if let Some(e) = this.guidelines.editor.as_mut() {
                                e.compare = !e.compare;
                            }
                            cx.notify();
                        })))
                        .child(ui::button("gl-discard", "Discard mine", ButtonKind::Outline, None).h(px(26.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.guideline_take_newest(cx)))),
                )
                .when(compare, |el| match theirs {
                    Some(theirs) => el.child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(format!("Version {newer} → yours"))).child(diff_box(&theirs, &text, mono.clone())),
                    None => el.child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Loading…")),
                })
        });
        let message = editor_message(failure, problem, &title, n);
        let main = v().flex_1().min_w_0().gap(px(12.))
            .child(back)
            .when_some(waiting.filter(|_| e.proposal.is_none()), |el, (pid, who)| {
                el.child(
                    h().id("gl-waiting").gap(px(8.)).px(px(12.)).py(px(8.)).rounded(px(7.)).border_1().border_color(c(theme::ACCENT)).cursor_pointer().text_size(px(12.5))
                        .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.guideline_show_proposal(Some(pid), w, cx)))
                        .child(ui::diamond(8., theme::ACCENT_TEXT, false))
                        .child(format!("{who} proposes a change · Review ›")),
                )
            })
            .when(e.archived, |el| el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child("Archived: no run gets this. Restore it from the library to change it.")))
            .when(!self.guidelines_can_edit(), |el| el.child(div().text_size(px(12.5)).text_color(c(theme::MUTED)).child(READ_ONLY)))
            .child(top)
            .child(tabs)
            .child(if e.loaded { panes.into_any_element() } else { div().text_color(c(theme::MUTED)).child("Loading…").into_any_element() })
            .child(counters)
            .children(conflict)
            .children(message.map(|m| div().text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::NEGATIVE)).child(m)))
            .child(footer);
        let history = e.history.then(|| self.render_guideline_history(&e, &text, cx));
        let page = h().items_start().gap(px(20.)).child(main).children(history);
        self.page(v().pt(px(24.)).pb(px(40.)).px(px(28.)).child(page)).into_any_element()
    }

    fn render_guideline_history(&self, e: &Editor, now: &str, cx: &mut Context<Self>) -> Div {
        let names = self.gl_names();
        let mono = ui::mono(cx);
        let can_edit = self.guidelines_can_edit() && !e.archived && e.proposal.is_none();
        let me = self.active_member().map(|u| u.id);
        let title = self.guidelines.title.read(cx).value().to_string();
        let panel = v().w(px(380.)).flex_none().gap(px(8.)).p(px(14.)).rounded(px(9.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
            .child(ui::eyebrow(format!("HISTORY · {}", title.to_uppercase())));
        let Some(id) = e.guideline else { return panel };
        let versions = match self.guidelines.versions.get(&id) {
            None => return panel.child(div().text_size(px(12.)).text_color(c(theme::MUTED)).child("Loading…")),
            Some(Err(why)) => return panel.child(div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(why.clone())),
            Some(Ok(v)) => v.clone(),
        };
        let current = versions.first().map_or(0, |v| v.version);
        let pick = e.pick;
        let rows: Vec<Stateful<Div>> = versions.iter().enumerate().map(|(i, ver)| {
            let version = ver.version;
            let is_current = version == current;
            let who = match &ver.by {
                Some(u) if Some(u.id) == me => Some("You".to_string()),
                Some(u) => Some(u.username.clone()),
                None => None,
            };
            let line = [Some(format!("Version {version}")), is_current.then(|| "current".to_string()), who, Some(ui::fmt_day(ver.created_at_ms))].into_iter().flatten().collect::<Vec<_>>().join(" · ");
            v().id(("gl-version", i)).gap(px(2.)).px(px(10.)).py(px(7.)).rounded(px(6.)).cursor_pointer()
                .border_1().border_color(c(if pick == Some(version) { theme::ACCENT } else { theme::BORDER }))
                .hover(|s| s.bg(c(theme::SURFACE_2)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if let Some(e) = this.guidelines.editor.as_mut() {
                        e.pick = if e.pick == Some(version) { None } else { Some(version) };
                    }
                    cx.notify();
                }))
                .child(
                    h().gap(px(8.)).text_size(px(12.))
                        .child(div().flex_1().text_color(c(if is_current { theme::ACCENT_TEXT } else { theme::TEXT_2 })).child(line))
                        .when(!is_current && can_edit, |el| {
                            el.child(ui::button(("gl-restore-version", i), "Restore", ButtonKind::Outline, None).h(px(24.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                                cx.stop_propagation();
                                this.guideline_dialog(Dialog::Confirm(Confirm::RestoreVersion(id, version)), w, cx);
                            })))
                        }),
                )
                .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(source_label(ver, &self.guidelines.proposals, names)))
        }).collect();
        let diff = pick.and_then(|p| versions.iter().find(|v| v.version == p)).map(|old| {
            v().gap(px(4.))
                .child(div().text_size(px(11.5)).text_color(c(theme::MUTED)).child(format!("Version {} → what the editor holds", old.version)))
                .child(diff_box(&old.text, now, mono.clone()))
        });
        panel.child(v().id("gl-history-list").max_h(px(320.)).overflow_y_scrollbar().gap(px(4.)).children(rows))
            .child(div().text_size(px(11.)).text_color(c(theme::MUTED)).child("Links have no versions: only the title and the text do."))
            .children(diff)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::test::TestWindowExt as _;
    use gpui_kit::{TestAppContext, WindowBounds, WindowOptions};
    // The glob of gpui_kit brings GPUI's `test`; the attribute it expands to is std's.
    use std::prelude::rust_2024::test;

    struct Panes {
        text: Entity<TextareaState>,
        mode: Mode,
    }

    impl Render for Panes {
        fn render(&mut self, window: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            let height = pane_height(f32::from(window.viewport_size().height));
            v().size_full().p(px(20.)).child(panes(self.mode, write_pane(&self.text, height, "monospace".into()), preview_pane(height, "## Commit messages")))
        }
    }

    /// Pulse M1: in Write and in Side by side the text box is as tall as the preview, the
    /// height the window leaves; it was one and a half lines.
    #[gpui_kit::test]
    fn the_text_box_is_as_tall_as_the_preview(cx: &mut TestAppContext) {
        cx.update(gpui_kit::init);
        for (mode, viewport) in [(Mode::Side, 900.), (Mode::Write, 900.), (Mode::Side, 1200.)] {
            let (window, _) = cx.update(|cx| {
                let bounds = Bounds { origin: Point::default(), size: size(px(1200.), px(viewport)) };
                gpui_kit::open_window(WindowOptions { window_bounds: Some(WindowBounds::Windowed(bounds)), ..Default::default() }, cx, |window, cx| {
                    let text = cx.new(|cx| TextareaState::new(window, cx).rows(22));
                    text.update(cx, |t, cx| t.set_value("## Commit messages\n- Write the subject in the imperative.", window, cx));
                    cx.new(|_| Panes { text, mode })
                })
                .unwrap_or_else(|e| panic!("window: {e}"))
            });
            cx.update_window(window, |_, window, cx| {
                window.render_frame(cx);
                let height = px(pane_height(viewport));
                let write = window.find("gl-write");
                assert!(write.visible(), "{mode:?}");
                assert_eq!(write.bounds().size.height, height, "{mode:?} in a window {viewport} high");
                assert!(write.bounds().size.width > px(300.), "{mode:?}: {:?}", write.bounds());
                if mode == Mode::Side {
                    let preview = window.find("gl-preview");
                    assert_eq!(preview.bounds().size.height, write.bounds().size.height, "side by side, one height");
                    assert_eq!(preview.bounds().top(), write.bounds().top());
                }
            })
            .unwrap_or_else(|e| panic!("update: {e}"));
        }
    }
}
