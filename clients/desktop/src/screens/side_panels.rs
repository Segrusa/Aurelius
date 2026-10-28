//! Side panels that slide in from the right: a screenshot the agent took, and
//! one item of "Running now" (an answer with its live text and tools, or a task).

use crate::app::{AppView, ManagerRef, Overlay, ProjectRef, Page, Tab};
use crate::keymap::*;
use crate::theme::{self, c};
use crate::resize::Edge;
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::ConversationId;
use aurelius_protocol::JobSummary;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use gpui_kit::component::scroll::ScrollableElement as _;
use std::sync::Arc;

/// A screenshot opened in the side panel (loaded from the server).
pub struct ShotView {
    pub id: String,
    pub image: Option<Arc<Image>>,
    pub error: Option<String>,
    /// A file kept with a message (its name), not an agent's screenshot.
    pub file: Option<String>,
}

/// The image format of these bytes, if the panel can show them.
pub fn image_format(bytes: &[u8]) -> Option<ImageFormat> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some(ImageFormat::Png),
        [0xFF, 0xD8, ..] => Some(ImageFormat::Jpeg),
        [b'G', b'I', b'F', ..] => Some(ImageFormat::Gif),
        [b'R', b'I', b'F', b'F', _, _, _, _, b'W', b'E', b'B', b'P', ..] => Some(ImageFormat::Webp),
        [b'B', b'M', ..] => Some(ImageFormat::Bmp),
        _ => None,
    }
}

/// A file name the app shows as a picture.
pub fn is_image_name(name: &str) -> bool {
    matches!(std::path::Path::new(name).extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(), Some("png" | "jpg" | "jpeg" | "gif" | "webp" | "bmp"))
}

impl AppView {
    /// Open a screenshot the agent took in the side panel.
    pub fn open_screenshot(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.requests_handle().send(crate::backend::Request::LoadScreenshot(id.clone()));
        self.shot = Some(ShotView { id, image: None, error: None, file: None });
        self.overlay = Overlay::Screenshot;
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// Open a file kept with a message: an image in this panel, anything else
    /// with this computer's own program (fetched from the server first).
    pub fn open_message_file(&mut self, file: aurelius_protocol::FileRef, window: &mut Window, cx: &mut Context<Self>) {
        let _ = self.requests_handle().send(crate::backend::Request::LoadFile(file.id.clone()));
        if file.media_type.starts_with("image/") && file.media_type != "image/svg+xml" {
            self.shot = Some(ShotView { id: file.id, image: None, error: None, file: Some(file.name) });
            self.overlay = Overlay::Screenshot;
            self.overlay_focus.focus(window, cx);
        } else {
            self.opening_files.insert(file.id);
            self.notify_user(format!("Opening {}…", file.name), false, cx);
        }
        cx.notify();
    }

    /// The server fetches uploaded files by id (`upload.get`).
    pub fn uploads_fetchable(&self) -> bool {
        matches!(&self.connection, crate::backend::Connection::Online { server, .. } if server.uploads)
    }

    /// Ask the server for the thumbnails of uploaded images in the tasks, recurring
    /// tasks, open project's ideas and open conversation that are not loaded yet.
    pub fn request_upload_thumbs(&mut self) {
        if !self.uploads_fetchable() {
            return;
        }
        let files = self.tasks.iter().flat_map(|t| &t.attachments)
            .chain(self.schedules.iter().flat_map(|s| &s.attachments))
            .chain(self.ideas.iter().flat_map(|(_, l)| l).flat_map(|i| &i.attachments))
            .chain(self.messages.iter().flat_map(|m| &m.messages).flat_map(|m| &m.uploads));
        let wanted: Vec<String> = files
            .filter(|f| is_image_name(&f.name))
            .filter_map(|f| f.id.clone())
            .filter(|id| !self.upload_thumbs.contains_key(id))
            .collect();
        for id in wanted {
            self.upload_thumbs.insert(id.clone(), None);
            let _ = self.requests_handle().send(crate::backend::Request::LoadUploadThumb(id));
        }
    }

    /// Open a file the user uploaded through the server (it may be on another
    /// computer): an image in the side panel, anything else with this computer's own program.
    pub fn open_upload_file(&mut self, file: &aurelius_protocol::MessageFileView, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = file.id.clone().filter(|_| self.uploads_fetchable()) else { return };
        let _ = self.requests_handle().send(crate::backend::Request::LoadUpload(id.clone()));
        if is_image_name(&file.name) {
            self.shot = Some(ShotView { id, image: None, error: None, file: Some(file.name.clone()) });
            self.overlay = Overlay::Screenshot;
            self.overlay_focus.focus(window, cx);
        } else {
            self.opening_files.insert(id);
            self.notify_user(format!("Opening {}…", file.name), false, cx);
        }
        cx.notify();
    }

    /// The server runs on this computer, so its folders are ours too.
    pub fn server_is_local(&self) -> bool {
        matches!(&self.connection, crate::backend::Connection::Online { server, .. } if !server.remote)
    }

    /// Open a file the user sent to the open manager conversation: `relative`
    /// lies in its work folder (the task's folder, else the manager's, else the
    /// project's), which is on this computer when the server is local.
    pub fn open_upload(&mut self, relative: &str, cx: &mut Context<Self>) {
        if !self.server_is_local() {
            return;
        }
        let Some(conversation) = self.current_manager_thread() else { return };
        let (manager, project_id) = match self.thread_screen() {
            crate::app::Page::Employee(manager) => (self.real_manager(manager), self.thread_project()),
            _ => (None, None),
        };

        let project = project_id.and_then(|id| self.projects.iter().find(|p| p.id == id)).and_then(|p| p.folder.clone());
        let found = [manager.and_then(|m| m.folder.clone()), project]
            .into_iter()
            .flatten()
            .map(|folder| std::path::Path::new(&folder).join(relative))
            .find(|path| path.is_file());
        match found {
            Some(path) => cx.open_with_system(&path),
            None => self.notify_user(format!("{relative} is no longer in the work folder."), true, cx),
        }
    }

    /// A file kept with a task or recurring task, in its work folder (the task's folder, else the
    /// manager's, else the project's); only found when the server runs on this computer.
    pub fn work_file(&self, project: aurelius_domain::ProjectId, manager: Option<aurelius_domain::EmployeeId>, task_folder: Option<String>, relative: &str) -> Option<std::path::PathBuf> {
        if !self.server_is_local() {
            return None;
        }
        let manager_folder = manager.and_then(|m| self.managers.iter().find(|x| x.id == m)).and_then(|m| m.folder.clone());
        let project_folder = self.projects.iter().find(|p| p.id == project).and_then(|p| p.folder.clone());
        [task_folder, manager_folder, project_folder]
            .into_iter()
            .flatten()
            .map(|folder| std::path::Path::new(&folder).join(relative))
            .find(|path| path.is_file())
    }

    /// A loaded file: shown in the panel, or saved to a temporary folder and opened.
    pub fn file_loaded(&mut self, id: String, result: Result<(String, String, Vec<u8>), String>, cx: &mut Context<Self>) {
        if let Some(shot) = self.shot.as_mut().filter(|s| s.id == id) {
            match result {
                Ok((_, _, bytes)) => match image_format(&bytes) {
                    Some(format) => shot.image = Some(Arc::new(Image::from_bytes(format, bytes))),
                    None => shot.error = Some("This image can't be shown here.".into()),
                },
                Err(error) => shot.error = Some(error),
            }
            cx.notify();
            return;
        }
        if !self.opening_files.remove(&id) {
            return;
        }
        match result {
            Ok((name, _, bytes)) => {
                let safe: String = name.chars().map(|c| if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') { '_' } else { c }).collect();
                let dir = std::env::temp_dir().join("aurelius-files").join(&id);
                let path = dir.join(if safe.trim().is_empty() { "file".to_string() } else { safe });
                match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, &bytes)) {
                    Ok(()) => cx.open_with_system(&path),
                    Err(e) => self.notify_user(format!("Could not open {name}: {e}"), true, cx),
                }
            }
            Err(error) => self.notify_user(error, true, cx),
        }
    }

    /// Show a conversation where it lives: with its project or its employee.
    pub fn open_conversation_anywhere(&mut self, conversation: ConversationId, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = Overlay::None;
        // A task's conversation and an employee's chat open on the employee's page.
        let manager = self.tasks.iter().find(|t| t.conversation_id == Some(conversation)).and_then(|t| t.employee_id)
            .or_else(|| self.managers.iter().find(|m| m.conversation_id == conversation || m.conversations.iter().any(|t| t.id == conversation)).map(|m| m.id));
        match manager {
            Some(manager) => {
                self.navigate(Page::Employee(ManagerRef::Real(manager)));
                self.select_manager_thread(conversation, cx);
            }
            None => self.notify_user("This conversation is no longer available.", false, cx),
        }
        self.focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn render_screenshot_panel(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let body: AnyElement = match self.shot.as_ref() {
            Some(ShotView { image: Some(image), .. }) => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(img(image.clone()).max_w_full().max_h_full().object_fit(ObjectFit::Contain).rounded(px(6.)))
                .into_any_element(),
            Some(ShotView { error: Some(error), .. }) => div().text_color(c(theme::MUTED)).child(error.clone()).into_any_element(),
            _ => div().text_color(c(theme::MUTED)).child("Loading…").into_any_element(),
        };
        let file_name = self.shot.as_ref().and_then(|s| s.file.clone());
        let panel = self.side_panel(if file_name.is_some() { "FILE" } else { "SCREENSHOT" }, cx)
            .child(div().id("shot-body").flex_1().min_h_0().p(px(20.)).child(body))
            .child(
                h().px(px(28.)).py(px(12.)).gap(px(10.)).border_t_1().border_color(c(theme::BORDER))
                    .child(h().flex_1().child(ui::info("shot-about", match &file_name {
                        Some(name) => format!("{name} · kept with the message on the server."),
                        None => "What the agent saw, scaled down as it was sent. Kept on the server (the last 300).".to_string(),
                    })))
                    .child(ui::kbd("escape", cx).opacity(0.6)),
            );
        self.slide_in(panel, ("shot-scrim", "shot-slide"), ("shot_panel", 900.), cx)
    }

    /// The panel's frame: header with eyebrow and close button.
    pub(crate) fn side_panel(&self, eyebrow: &'static str, cx: &App) -> Div {
        v().size_full()
            .bg(c(theme::PANEL))
            .border_l_1()
            .border_color(c(theme::BORDER_STRONG))
            .shadow_lg()
            .track_focus(&self.overlay_focus)
            .key_context(SIDE_PANEL)
            .child(
                h().px(px(28.)).py(px(14.)).gap(px(10.)).border_b_1().border_color(c(theme::BORDER))
                    .child(ui::diamond(9., theme::ACCENT_TEXT, false))
                    .child(div().flex_1().child(ui::eyebrow(eyebrow)))
                    .child(ui::close_button(cx)),
            )
    }

    /// Dim the window and slide the panel in from the right; a click beside it closes it.
    pub(crate) fn slide_in(&self, panel: Div, ids: (&'static str, &'static str), width: (&'static str, f32), cx: &mut Context<Self>) -> AnyElement {
        let key = self.overlay_key();
        let panel = self.resizable(width.0, width.1, Edge::Left, panel, cx)
            .max_w(relative(0.92))
            // A click on the panel (or its edge) doesn't close it.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation());
        deferred(ui::fade_in(
            div()
                .id(ids.0)
                .absolute()
                .top_0()
                .left_0()
                .size_full()
                .bg(Hsla { a: 0.45, ..c(0x000000) })
                .flex()
                .justify_end()
                .occlude()
                .on_mouse_down(MouseButton::Left, cx.listener(|_, _, w, cx| w.dispatch_action(Box::new(Back), cx)))
                .child(panel.with_animation((ids.1, key), Animation::new(std::time::Duration::from_millis(280)), |el, t| {
                    let e = 1.0 - (1.0 - t).powi(4);
                    el.opacity(e).relative().left(px((1.0 - e) * 56.0))
                })),
            (ids.0, key),
            140,
        ))
        .with_priority(1)
        .into_any_element()
    }
}
