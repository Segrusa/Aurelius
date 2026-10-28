//! Edit project (project menu, palette, Ctrl+Shift+, and F2): name, description and folder.
//! A project's plugins are under Settings → Advanced → Plugins.

use crate::app::AppView;
use crate::backend;
use crate::keymap::{LOAD, OpenProjectFolder};
use crate::screens::servers::FolderPurpose;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::ProjectId;
use aurelius_domain::project::DESCRIPTION_MAX;
use gpui_kit::component::input::{InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;

pub struct ProjectSettings {
    pub project: ProjectId,
    pub name: Entity<InputState>,
    pub description: Entity<TextareaState>,
    /// Waiting for `project.set_description` to answer.
    pub description_saving: bool,
    pub description_error: Option<String>,
}

/// The description as the server stores it, or why it can't be saved.
fn description_to_save(text: &str) -> Result<String, String> {
    let text = text.trim();
    let count = text.chars().count();
    if count > DESCRIPTION_MAX {
        return Err(format!("Too long: {count} of at most {DESCRIPTION_MAX} characters."));
    }
    Ok(text.to_string())
}

fn label(text: &'static str) -> Div {
    div().w(px(150.)).flex_none().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(text)
}

/// A labelled row that wraps under its label when the card is narrow.
fn row(text: &'static str, value: impl IntoElement) -> Div {
    h().flex_wrap().gap(px(8.)).items_start().child(label(text)).child(div().flex_1().min_w(px(240.)).child(value))
}

impl AppView {
    pub fn open_project_settings(&mut self, project: ProjectId, window: &mut Window, cx: &mut Context<Self>) {
        let Some((name, description)) = self.projects.iter().find(|p| p.id == project).map(|p| (p.name.clone(), p.description.clone().unwrap_or_default())) else { return };
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Project name"));
        input.update(cx, |i, cx| i.set_value(name, window, cx));
        let description_input = cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 10).placeholder("What this project is about"));
        description_input.update(cx, |i, cx| i.set_value(description, window, cx));
        // Re-render for the character count.
        cx.observe(&description_input, |_, _, cx| cx.notify()).detach();
        cx.subscribe_in(&input, window, |this, _, event: &InputEvent, _, cx| {
            if matches!(event, InputEvent::PressEnter { .. }) {
                this.project_settings_rename(cx);
            }
        })
        .detach();
        self.project_settings = Some(ProjectSettings { project, name: input, description: description_input, description_saving: false, description_error: None });
        cx.notify();
    }

    pub fn close_project_settings(&mut self, cx: &mut Context<Self>) {
        self.project_settings = None;
        cx.notify();
    }

    fn project_settings_rename(&mut self, cx: &mut Context<Self>) {
        let Some(s) = &self.project_settings else { return };
        let name = s.name.read(cx).value().trim().to_string();
        let Some(p) = self.projects.iter().find(|p| p.id == s.project) else { return };
        if name.is_empty() || name == p.name {
            return;
        }
        let _ = self.requests_handle().send(backend::Request::RenameProject { id: p.id, name, expected_version: p.version });
        cx.notify();
    }

    fn project_settings_save_description(&mut self, cx: &mut Context<Self>) {
        let Some(s) = &self.project_settings else { return };
        let Some(p) = self.projects.iter().find(|p| p.id == s.project) else { return };
        if s.description_saving {
            return;
        }
        let (id, version, current) = (p.id, p.version, p.description.clone().unwrap_or_default());
        let result = description_to_save(&s.description.read(cx).value());
        let send = matches!(&result, Ok(text) if text != current.trim());
        if let Some(s) = self.project_settings.as_mut() {
            s.description_saving = send;
            s.description_error = result.as_ref().err().cloned();
        }
        if let (true, Ok(description)) = (send, result) {
            let _ = self.requests_handle().send(backend::Request::SetProjectDescription { id, description, expected_version: version });
        }
        cx.notify();
    }

    fn project_settings_reset_description(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(s) = self.project_settings.as_mut() else { return };
        let current = self.projects.iter().find(|p| p.id == s.project).and_then(|p| p.description.clone()).unwrap_or_default();
        s.description_error = None;
        s.description.update(cx, |i, cx| i.set_value(current, window, cx));
        cx.notify();
    }

    /// `Update::DescriptionSaved`: on success the project list brings the new text; on failure keep the edit and say why.
    pub fn project_settings_description_saved(&mut self, project: ProjectId, result: Result<(), String>, cx: &mut Context<Self>) {
        let Some(s) = self.project_settings.as_mut().filter(|s| s.project == project) else { return };
        s.description_saving = false;
        s.description_error = result.err();
        cx.notify();
    }

    pub fn render_project_settings(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(s) = &self.project_settings else { return div().into_any_element() };
        let Some(p) = self.projects.iter().find(|p| p.id == s.project).cloned() else { return div().into_any_element() };
        let name_input = s.name.clone();
        let top = 70.;
        let max_h = (f32::from(window.viewport_size().height) - top - 24.).max(160.);
        let max_w = (f32::from(window.viewport_size().width) - 32.).max(280.);
        let scroll = window.use_keyed_state("project-settings-scroll", cx, |_, _| ScrollHandle::default()).read(cx).clone();
        let section = |title: &'static str| div().pt(px(6.)).child(ui::eyebrow(title));
        let muted = |text: String| div().text_size(px(12.5)).line_height(px(18.)).text_color(c(theme::TEXT_2)).child(text);

        let folder = p.folder.clone().unwrap_or_else(|| "No folder".into());
        let general = v().gap(px(12.))
            .child(section("GENERAL"))
            .child(row("Name", h().gap(px(8.))
                .child(div().flex_1().min_w_0().child(ui::input(&name_input)))
                .child(ui::button("project-settings-rename", "Save", ButtonKind::Outline, None).h(px(30.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.project_settings_rename(cx))))))
            .child(row("Description", if self.project_description_supported() {
                let count = s.description.read(cx).value().trim().chars().count();
                let over = count > DESCRIPTION_MAX;
                let saving = s.description_saving;
                v().gap(px(6.))
                    .child(Textarea::new(&s.description).py(px(7.)).line_height(px(20.)))
                    .child(h().gap(px(8.)).items_center()
                        .child(div().flex_1().text_size(px(12.)).text_color(c(if over { theme::NEGATIVE } else { theme::TEXT_2 })).child(format!("{count} / {DESCRIPTION_MAX}")))
                        .child(ui::button("project-settings-description-cancel", "Cancel", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.project_settings_reset_description(w, cx))))
                        .child(ui::button("project-settings-description-save", if saving { "Saving…" } else { "Save" }, ButtonKind::Outline, None).h(px(28.)).when(over || saving, |b| b.opacity(0.5))
                            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.project_settings_save_description(cx)))))
                    .when_some(s.description_error.clone(), |el, e| el.child(div().text_size(px(12.5)).text_color(c(theme::NEGATIVE)).child(e)))
                    .into_any_element()
            } else {
                v().gap(px(4.))
                    .child(muted(p.description.clone().filter(|d| !d.trim().is_empty()).unwrap_or_else(|| "No description".into())))
                    .child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("This server is too old to edit the description; update it to change it here."))
                    .into_any_element()
            }))
            .child(row("Folder", v().gap(px(6.))
                .child(div().text_size(px(12.5)).font_family(ui::mono(cx)).child(folder))
                .child(h().flex_wrap().gap(px(8.))
                    .child(ui::button("project-settings-open-folder", "Open folder", ButtonKind::Outline, None).h(px(28.)).on_click(|_: &ClickEvent, w, cx| w.dispatch_action(Box::new(OpenProjectFolder), cx)))
                    .child(ui::button("project-settings-folder", "Choose another folder…", ButtonKind::Outline, None).h(px(28.)).on_click(cx.listener(move |this, _: &ClickEvent, w, cx| {
                        // The folder browser is an overlay beneath this card.
                        this.close_project_settings(cx);
                        this.pick_folder(FolderPurpose::ProjectFolder(p.id), "Use as project folder", w, cx);
                    }))))));

        let body = v().gap(px(14.))
            .child(div().font_family(ui::serif(cx)).text_size(px(22.)).child(format!("Edit project · {}", p.name)))
            .child(general);

        let card = crate::screens::overlays::card(720.)
            .child(ui::scroll_frame(&scroll, crate::screens::overlays::scroll_body("project-settings-body", &scroll, (max_h - 70.).max(120.)).p(px(22.)).child(body)))
            .child(
                h().px(px(22.)).py(px(14.)).gap(px(8.)).border_t_1().border_color(c(theme::BORDER)).justify_end()
                    .child(ui::button("project-settings-close", "Close", ButtonKind::Outline, Some(ui::kbd("escape", cx))).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.close_project_settings(cx)))),
            );
        deferred(
            div().id("project-settings-scrim").absolute().top_0().left_0().size_full().bg(Hsla { a: 0.55, ..c(0x000000) }).flex().justify_center().items_start().pt(px(top)).occlude()
                .key_context(LOAD)
                .child(v().max_h(px(max_h)).max_w(px(max_w)).on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation()).child(card)),
        )
        .with_priority(1)
        .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{DESCRIPTION_MAX, description_to_save};

    #[test]
    fn description_is_trimmed_and_limited() {
        assert_eq!(description_to_save("  Atlas  \n").unwrap(), "Atlas");
        assert_eq!(description_to_save("   ").unwrap(), "");
        assert!(description_to_save(&"é".repeat(DESCRIPTION_MAX)).is_ok());
        assert!(description_to_save(&"é".repeat(DESCRIPTION_MAX + 1)).unwrap_err().contains("2001"));
    }
}
