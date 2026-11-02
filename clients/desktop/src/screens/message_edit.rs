//! Edit and delete your own messages (AUR-FA A6, ui-ontwerp §7, freeze §5): in the Team chat's
//! channels and threads and in a ticket's discussion. The server decides who may
//! (`MessageView::editable`, per reader); the client only hides what the server says no to, and
//! everything is behind the feature `message_edits`. An edit wakes no one and an employee that
//! read the message keeps the original; a delete leaves a line. Neither erases a secret: the words
//! of both dialogs say so. The text a board member types goes through the vault redactor on the
//! server and is never logged here.

use crate::app::{AppView, ConfirmKind, MenuDo, MenuEntry, Overlay};
use crate::backend::{self, BoardCall};
use crate::screens::discussion as d;
use crate::screens::thread_panel::ChatDo;
use crate::theme::{self, c};
use crate::ui::{self, ButtonKind, h, v};
use aurelius_domain::group_chat::{DeleteMessage, EditMessage};
use aurelius_domain::views::{MessageHistoryView, MessageView};
use aurelius_domain::{Command, MessageId};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// What a deleted message says in place of its text (freeze §5).
pub const DELETED: &str = "This message was deleted.";
/// The title of the delete dialog (B3, word for word).
pub const DELETE_TITLE: &str = "Delete this message?";
/// The body of the delete dialog (B3, word for word).
pub const DELETE_BODY: &str = "A line stays that says it was deleted. The text stays in the audit log and backups, and employees that read it keep it. This doesn't erase a secret: rotate it and put it in the vault.";
/// Under the edit field (B3, word for word).
pub const EDIT_HINT: &str = "Editing wakes no one. An employee that already read this keeps the original. This doesn't erase a secret: rotate it and put it in the vault.";
/// Save is off for an empty text.
pub const EMPTY_NOTE: &str = "To remove it, delete the message.";
/// A message that cannot be edited any more while its field was open (deleted, or its channel closed).
pub const NOT_EDITABLE: &str = "This message can't be edited any more.";
/// The text the field shows while it loads its earlier versions.
pub const HISTORY_NONE: &str = "No earlier versions.";
/// The earlier texts show cut to this many characters.
pub const HISTORY_TEXT_MAX: usize = 400;

/// What Save does with the text in the field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Save {
    /// Nothing typed: Save is off.
    Off,
    /// Nothing changed: the field closes without a write.
    Close,
    /// The text to send.
    Send(String),
}

/// What Save does for `new` over the message's `original` text.
pub fn save_of(original: &str, new: &str) -> Save {
    let new = new.trim();
    if new.is_empty() {
        Save::Off
    } else if new == original.trim() {
        Save::Close
    } else {
        Save::Send(new.to_string())
    }
}

/// Whether the message can be edited or deleted here: the server said so (`editable`), and the
/// server has the feature. A deleted message never can.
pub fn can_change(m: &MessageView, supported: bool) -> bool {
    supported && m.editable && m.deleted_at_ms.is_none()
}

/// Whether a message was deleted (its row shows the placeholder, no text, no files).
pub fn is_deleted(m: &MessageView) -> bool {
    m.deleted_at_ms.is_some()
}

/// Whether a row says "edited": it was, and the server has the feature to show it.
pub fn shows_edited(m: &MessageView, supported: bool) -> bool {
    supported && m.edited_at_ms.is_some() && m.deleted_at_ms.is_none()
}

/// The names `new` mentions that `original` did not and that someone here carries: an edit tells
/// them nothing (P-92).
pub fn new_mentions(original: &str, new: &str, people: &[&str]) -> Vec<String> {
    let before = d::mentions(original);
    d::mentions(new)
        .into_iter()
        .filter(|n| !before.iter().any(|b| b.eq_ignore_ascii_case(n)))
        .filter_map(|n| people.iter().find(|p| p.eq_ignore_ascii_case(&n)).map(|p| (*p).to_string()))
        .collect()
}

/// "@Bram won't be notified by an edit. Send a new message to reach them."
pub fn mention_note(names: &[String]) -> Option<String> {
    if names.is_empty() {
        return None;
    }
    let list = names.iter().map(|n| format!("@{n}")).collect::<Vec<_>>().join(", ");
    Some(format!("{list} won't be notified by an edit. Send a new message to reach {}.", if names.len() == 1 { "them" } else { "them all" }))
}

/// What a screen reader says of a row: "Message by Alexandra at 10:02, edited." and
/// "Deleted message by Alexandra at 10:05." Whatever else the row says is not changed.
pub fn row_label(who: &str, when: &str, m: &MessageView, supported: bool) -> Option<String> {
    if is_deleted(m) {
        Some(format!("Deleted message by {who} at {when}."))
    } else if shows_edited(m, supported) {
        Some(format!("Message by {who} at {when}, edited."))
    } else {
        None
    }
}

/// An earlier text, cut so a long one does not fill the row.
pub fn history_line(text: &str) -> String {
    let text = text.trim();
    if text.chars().count() <= HISTORY_TEXT_MAX {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(HISTORY_TEXT_MAX).collect::<String>())
    }
}

/// The message being edited, and what the window knows around it.
pub struct MessageEdit {
    pub input: Entity<TextareaState>,
    /// The message in the field, with its text as it was when the field opened.
    pub editing: Option<(MessageId, String)>,
    /// Save went; waiting for the answer.
    pub busy: bool,
    /// Why Save did not work, shown under the field; the text stays.
    pub error: Option<String>,
    /// The message whose earlier versions show under it, and what the server said.
    pub history: Option<(MessageId, Option<Result<MessageHistoryView, String>>)>,
}

impl MessageEdit {
    pub fn new(window: &mut Window, cx: &mut Context<AppView>) -> Self {
        Self {
            input: cx.new(|cx| TextareaState::new(window, cx).auto_grow(2, 10).submit_on_enter(true).placeholder("Edit the message")),
            editing: None,
            busy: false,
            error: None,
            history: None,
        }
    }

    /// Enter saves, Shift+Enter is a new line.
    pub fn subscriptions(&self, window: &mut Window, cx: &mut Context<AppView>) -> Vec<Subscription> {
        vec![cx.subscribe_in(&self.input, window, |this, _, e: &InputEvent, w, cx| match e {
            InputEvent::PressEnter { shift: false, .. } => this.edit_save(w, cx),
            InputEvent::Change => {
                this.edit.error = None;
                cx.notify();
            }
            _ => {}
        })]
    }

    /// Whether this message is in the field.
    pub fn is(&self, id: MessageId) -> bool {
        self.editing.as_ref().is_some_and(|(e, _)| *e == id)
    }
}

impl AppView {
    /// This server has A6 (feature `message_edits`).
    pub fn message_edits_supported(&self) -> bool {
        matches!(&self.connection, backend::Connection::Online { server, .. } if server.message_edits)
    }

    /// A message of the open channel, thread or ticket discussion.
    pub fn edit_find(&self, id: MessageId) -> Option<MessageView> {
        self.shown_message(id).cloned().or_else(|| self.discussion_messages().and_then(|all| all.iter().find(|m| m.id == id)).cloned())
    }

    /// Whether the reader can edit or delete this message here.
    pub fn message_changeable(&self, m: &MessageView) -> bool {
        can_change(m, self.message_edits_supported())
    }

    /// Edit and Delete as menu entries; none when the server says the message is not the reader's
    /// to change. A message that waits in the queue keeps its own Delete.
    pub fn edit_menu_entries(&self, m: &MessageView) -> Vec<Option<MenuEntry>> {
        if !self.message_changeable(m) || m.queued {
            return Vec::new();
        }
        let entry = |label: &str, run, danger| Some(MenuEntry { label: label.to_string().into(), keys: None, run: MenuDo::Chat(run), danger, checked: false, detail: None });
        vec![entry("Edit", ChatDo::Edit(m.id), false), entry("Delete", ChatDo::Delete(m.id), true)]
    }

    /// Open the field on a message, with its text.
    pub fn edit_start(&mut self, id: MessageId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(m) = self.edit_find(id).filter(|m| self.message_changeable(m)) else { return };
        self.edit.editing = Some((id, m.text.clone()));
        self.edit.busy = false;
        self.edit.error = None;
        self.edit.input.update(cx, |input, cx| input.set_value(m.text.clone(), window, cx));
        self.edit.input.update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    /// Esc or Cancel: the field closes and the row is as it was.
    pub fn edit_cancel(&mut self, cx: &mut Context<Self>) {
        self.edit.editing = None;
        self.edit.busy = false;
        self.edit.error = None;
        cx.notify();
    }

    /// Whether Esc closes the field now.
    pub fn edit_open(&self) -> bool {
        self.edit.editing.is_some() && self.overlay == Overlay::None
    }

    /// Save: a changed text goes as `message.edit`; the same text only closes the field.
    pub fn edit_save(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some((id, original)) = self.edit.editing.clone() else { return };
        if self.edit.busy {
            return;
        }
        let text = self.edit.input.read(cx).value().to_string();
        match save_of(&original, &text) {
            Save::Off => {}
            Save::Close => self.edit_cancel(cx),
            Save::Send(text) => {
                if !matches!(self.connection, backend::Connection::Online { .. }) {
                    self.edit.error = Some("Offline · reconnecting".into());
                    cx.notify();
                    return;
                }
                self.edit.busy = true;
                self.edit.error = None;
                let command = Command::EditMessage(EditMessage { message_id: id, text, confirm_secret: false });
                let _ = self.requests.send(backend::Request::Board(BoardCall::Write { command, expected_version: None, done: None }));
                cx.notify();
            }
        }
    }

    /// "Send anyway" for an edit the server held back as a possible secret (B10).
    pub fn edit_sent_anyway(&mut self) {
        self.edit.busy = true;
        self.edit.error = None;
    }

    /// Delete asks first (B3).
    pub fn edit_delete_ask(&mut self, id: MessageId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.edit_find(id).is_some_and(|m| self.message_changeable(&m)) {
            return;
        }
        self.overlay = Overlay::Confirm(ConfirmKind::DeleteMessage(id));
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// The dialog's Delete.
    pub fn edit_delete(&mut self, id: MessageId, cx: &mut Context<Self>) {
        let command = Command::DeleteMessage(DeleteMessage { message_id: id });
        let _ = self.requests.send(backend::Request::Board(BoardCall::Write { command, expected_version: None, done: None }));
        cx.notify();
    }

    /// What the dialog adds when a ticket was made from the message: that ticket keeps its copy.
    pub fn delete_ticket_note(&self, id: MessageId) -> Option<String> {
        let all = self.messages.as_ref().map(|s| &s.messages[..])?;
        d::ticket_from(all, id).filter(|(_, number)| number.starts_with('#')).map(|(_, number)| format!("{number} keeps its copy."))
    }

    /// The answer of `message.edit` or `message.delete`.
    pub fn apply_message_written(&mut self, message: MessageId, deleted: bool, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) if !deleted => {
                if self.edit.is(message) {
                    self.edit_cancel(cx);
                }
            }
            Ok(()) => {
                if self.edit.is(message) {
                    self.edit_cancel(cx);
                }
                if self.edit.history.as_ref().is_some_and(|(m, _)| *m == message) {
                    self.edit.history = None;
                }
            }
            Err(why) if deleted => self.notify_user(why, true, cx),
            Err(why) => {
                if self.edit.is(message) {
                    self.edit.busy = false;
                    self.edit.error = Some(why);
                }
            }
        }
        cx.notify();
    }

    /// The row's "edited" was pressed: its earlier versions show under it, or hide again.
    pub fn edit_toggle_history(&mut self, id: MessageId, cx: &mut Context<Self>) {
        if self.edit.history.as_ref().is_some_and(|(m, _)| *m == id) {
            self.edit.history = None;
        } else {
            self.edit.history = Some((id, None));
            let _ = self.requests.send(backend::Request::Board(BoardCall::MessageHistory(id)));
        }
        cx.notify();
    }

    /// The answer of `message.history`.
    pub fn apply_message_history(&mut self, id: MessageId, result: Result<MessageHistoryView, String>, cx: &mut Context<Self>) {
        if let Some((open, slot)) = self.edit.history.as_mut().filter(|(m, _)| *m == id) {
            let _ = open;
            *slot = Some(result);
            cx.notify();
        }
    }

    /// The tail of a message's header: "edited", pressed to read the earlier versions.
    pub fn edited_tag(&self, m: &MessageView, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !shows_edited(m, self.message_edits_supported()) {
            return None;
        }
        let id = m.id;
        let when = m.edited_at_ms.map(|ms| format!("Edited {}", ui::fmt_when(ms))).unwrap_or_default();
        Some(
            div().id(SharedString::from(format!("edited-{id}"))).role(Role::Button).aria_label("Earlier versions").cursor_pointer().text_size(px(11.)).text_color(c(theme::TEXT_2)).hover(|s| s.text_color(c(theme::TEXT)))
                .tooltip(ui::tip(when, None))
                .child("edited")
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.edit_toggle_history(id, cx);
                }))
                .into_any_element(),
        )
    }

    /// Under a row: the earlier versions of its text, newest first, read-only.
    pub fn history_block(&self, m: &MessageView) -> Option<AnyElement> {
        let (id, slot) = self.edit.history.as_ref()?;
        if *id != m.id {
            return None;
        }
        let rows: Vec<AnyElement> = match slot {
            None => vec![div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child("Loading…").into_any_element()],
            Some(Err(why)) => vec![div().text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(why.clone()).into_any_element()],
            Some(Ok(view)) if view.versions.is_empty() => vec![div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(HISTORY_NONE).into_any_element()],
            Some(Ok(view)) => view
                .versions
                .iter()
                .map(|version| {
                    v().gap(px(2.))
                        .child(div().text_size(px(11.)).text_color(c(theme::TEXT_2)).child(format!("Replaced {}", ui::fmt_when(version.replaced_at_ms))))
                        .child(div().text_size(px(12.5)).line_height(px(18.)).child(history_line(&version.text)))
                        .into_any_element()
                })
                .collect(),
        };
        Some(
            v().id(SharedString::from(format!("history-{}", m.id))).role(Role::Group).aria_label("Earlier versions").gap(px(8.)).p(px(10.)).rounded(px(6.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                .child(div().text_size(px(11.)).text_color(c(theme::TEXT_2)).child("EARLIER VERSIONS"))
                .children(rows)
                .into_any_element(),
        )
    }

    /// What replaces a deleted message's text: a quiet, italic line, never a bubble.
    pub fn deleted_line(&self, m: &MessageView) -> AnyElement {
        div().id(SharedString::from(format!("deleted-{}", m.id))).role(Role::Note).text_size(px(13.)).italic().text_color(c(theme::TEXT_2)).child(format!("⊘ {DELETED}")).into_any_element()
    }

    /// The field in place of the text of the row being edited.
    pub fn edit_field(&self, m: &MessageView, cx: &mut Context<Self>) -> AnyElement {
        let id = m.id;
        let original = self.edit.editing.as_ref().map(|(_, o)| o.clone()).unwrap_or_default();
        let text = self.edit.input.read(cx).value().to_string();
        let save = save_of(&original, &text);
        let people = self.mentionables();
        let names: Vec<&str> = people.iter().map(|p| p.name.as_str()).collect();
        let note = mention_note(&new_mentions(&original, &text, &names));
        // Closed under it: deleted meanwhile, or its channel was closed.
        let gone = (!self.message_changeable(m)).then(|| if is_deleted(m) { DELETED } else { NOT_EDITABLE });
        let busy = self.edit.busy;
        let can_save = !matches!(save, Save::Off) && !busy && gone.is_none();
        v().id(SharedString::from(format!("edit-{id}"))).gap(px(6.))
            .child(
                div().rounded(px(8.)).border_1().border_color(c(theme::BORDER_STRONG)).bg(c(theme::SURFACE)).px(px(10.)).child(gpui_kit::component::input::Textarea::new(&self.edit.input).py(px(7.)).line_height(px(22.)).appearance(false)),
            )
            .child(div().text_size(px(11.5)).line_height(px(16.)).text_color(c(theme::TEXT_2)).child(EDIT_HINT))
            .when(matches!(save, Save::Off), |el| el.child(div().text_size(px(11.5)).text_color(c(theme::TEXT_2)).child(EMPTY_NOTE)))
            .children(note.map(|n| div().id(SharedString::from(format!("edit-mention-{id}"))).role(Role::Status).text_size(px(11.5)).text_color(c(theme::ACCENT_TEXT)).child(n)))
            .children(self.edit.error.clone().map(|e| div().id(SharedString::from(format!("edit-error-{id}"))).role(Role::Alert).text_size(px(12.)).text_color(c(theme::NEGATIVE)).child(e)))
            .children(gone.map(|why| {
                h().gap(px(8.)).text_size(px(12.)).text_color(c(theme::NEGATIVE))
                    .child(why)
                    .child(ui::button(SharedString::from(format!("edit-copy-{id}")), "Copy my text", ButtonKind::Outline, None).h(px(26.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        let text = this.edit.input.read(cx).value().to_string();
                        this.copy_text(format!("edit-{id}"), text, cx);
                        this.edit_cancel(cx);
                    })))
            }))
            .child(
                h().gap(px(8.)).justify_end()
                    .child(ui::button(SharedString::from(format!("edit-cancel-{id}")), "Cancel", ButtonKind::Outline, Some(ui::kbd("escape", cx))).h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.edit_cancel(cx))))
                    .child(
                        ui::button(SharedString::from(format!("edit-save-{id}")), if busy { "Saving…" } else { "Save" }, ButtonKind::Primary, None).h(px(28.)).when(!can_save, |b| b.opacity(0.5)).when(can_save, |b| {
                            b.on_click(cx.listener(|this, _: &ClickEvent, w, cx| this.edit_save(w, cx)))
                        }),
                    ),
            )
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    // Not `super::*`: gpui's `test` attribute would shadow the standard one.
    use super::{DELETE_BODY, DELETE_TITLE, DELETED, EDIT_HINT, HISTORY_TEXT_MAX, Save, can_change, history_line, is_deleted, mention_note, new_mentions, row_label, save_of, shows_edited};
    use aurelius_domain::views::{MessageVersionView, MessageView};

    fn message() -> MessageView {
        crate::screens::discussion::tests::msg(aurelius_domain::conversation::MessageRole::User, "hello")
    }

    #[test]
    fn the_dialog_says_what_the_freeze_says() {
        assert_eq!(format!("{DELETE_TITLE} {DELETE_BODY}"), "Delete this message? A line stays that says it was deleted. The text stays in the audit log and backups, and employees that read it keep it. This doesn't erase a secret: rotate it and put it in the vault.");
        assert_eq!(EDIT_HINT, "Editing wakes no one. An employee that already read this keeps the original. This doesn't erase a secret: rotate it and put it in the vault.");
        assert_eq!(DELETED, "This message was deleted.");
    }

    #[test]
    fn save_is_off_for_nothing_closes_for_the_same_text_and_sends_a_change() {
        assert_eq!(save_of("a", "   "), Save::Off);
        assert_eq!(save_of("a b", " a b "), Save::Close);
        assert_eq!(save_of("a", " b "), Save::Send("b".into()));
    }

    #[test]
    fn only_what_the_server_allows_is_offered_and_only_with_the_feature() {
        let mut m = message();
        assert!(!can_change(&m, true), "editable is false");
        m.editable = true;
        assert!(can_change(&m, true));
        assert!(!can_change(&m, false), "an older server: hidden");
        m.deleted_at_ms = Some(5);
        assert!(!can_change(&m, true), "a deleted message has no actions");
        assert!(is_deleted(&m));
    }

    #[test]
    fn edited_shows_only_with_the_feature_and_never_on_a_deleted_message() {
        let mut m = message();
        assert!(!shows_edited(&m, true));
        m.edited_at_ms = Some(9);
        assert!(shows_edited(&m, true));
        assert!(!shows_edited(&m, false));
        m.deleted_at_ms = Some(10);
        assert!(!shows_edited(&m, true));
    }

    #[test]
    fn a_new_mention_is_said_not_notified() {
        let people = ["Bram", "Forge"];
        assert_eq!(new_mentions("hi @Bram", "hi @Bram and @forge", &people), ["Forge"]);
        assert!(new_mentions("hi @Bram", "hi @bram", &people).is_empty(), "already there");
        assert!(new_mentions("hi", "mail me@x.org", &people).is_empty(), "not a person");
        assert_eq!(mention_note(&["Bram".into()]).as_deref(), Some("@Bram won't be notified by an edit. Send a new message to reach them."));
        assert_eq!(mention_note(&[]), None);
    }

    #[test]
    fn a_screen_reader_hears_edited_and_deleted() {
        let mut m = message();
        assert_eq!(row_label("Alexandra", "10:02", &m, true), None);
        m.edited_at_ms = Some(1);
        assert_eq!(row_label("Alexandra", "10:02", &m, true).as_deref(), Some("Message by Alexandra at 10:02, edited."));
        m.deleted_at_ms = Some(2);
        assert_eq!(row_label("Alexandra", "10:05", &m, true).as_deref(), Some("Deleted message by Alexandra at 10:05."));
    }

    #[test]
    fn a_long_earlier_text_is_cut() {
        let long = "x".repeat(HISTORY_TEXT_MAX + 50);
        assert_eq!(history_line(&long).chars().count(), HISTORY_TEXT_MAX + 1);
        assert_eq!(history_line("  short "), "short");
        let _ = MessageVersionView { text: String::new(), replaced_at_ms: 0 };
    }
}
