//! Live progress of an answer (like Claude Desktop): the run's TodoWrite list as a checklist,
//! step labels ("Running a command"), and the quick replies the agent ends an answer with
//! (feature `quick_replies`, tool `suggest_replies`; only suggestions, nothing waits for them).

use crate::app::AppView;
use crate::theme::{self, c};
use crate::ui::{self, h, v, ButtonKind};
use aurelius_domain::conversation::{MessageRole, MessageStatus};
use aurelius_protocol::ActivityView;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

pub const TODO_TOOL: &str = "TodoWrite";
pub const SUGGEST_TOOL: &str = "mcp__aurelius__suggest_replies";
/// Longest label a reply button shows.
const REPLY_LABEL_CHARS: usize = 40;
/// A longer checklist folds to the step in progress and a count.
const TODO_FOLD_ROWS: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TodoState {
    Done,
    Doing,
    Open,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Todo {
    pub text: String,
    pub state: TodoState,
}

/// The latest TodoWrite of an answer: only its last state counts. `None` without one or when empty.
pub fn todos(activity: &[ActivityView]) -> Option<Vec<Todo>> {
    let last = activity.iter().filter(|a| a.tool == TODO_TOOL).max_by_key(|a| a.offset)?;
    let list: Vec<Todo> = last.input["todos"]
        .as_array()?
        .iter()
        .filter_map(|t| {
            let state = match t["status"].as_str().unwrap_or("pending") {
                "completed" => TodoState::Done,
                "in_progress" => TodoState::Doing,
                _ => TodoState::Open,
            };
            // The active form ("Running tests") reads best while it is the step in progress.
            let text = match state {
                TodoState::Doing => t["activeForm"].as_str().or(t["content"].as_str()),
                _ => t["content"].as_str(),
            }?;
            Some(Todo { text: text.trim().to_owned(), state })
        })
        .filter(|t| !t.text.is_empty())
        .collect();
    (!list.is_empty()).then_some(list)
}

/// What the agent suggests answering: the question (if any) and 2-3 replies.
pub fn quick_replies(activity: &[ActivityView]) -> Option<(Option<String>, Vec<String>)> {
    let last = activity.iter().filter(|a| a.tool == SUGGEST_TOOL).max_by_key(|a| a.offset)?;
    let options: Vec<String> = last.input["options"]
        .as_array()?
        .iter()
        .filter_map(|o| o.as_str())
        .map(|o| o.trim().to_owned())
        .filter(|o| !o.is_empty())
        .take(3)
        .collect();
    if options.len() < 2 {
        return None;
    }
    let question = last.input["question"].as_str().map(str::trim).filter(|q| !q.is_empty()).map(str::to_owned);
    Some((question, options))
}

/// A reply as a button shows it.
pub fn reply_label(reply: &str) -> String {
    if reply.chars().count() <= REPLY_LABEL_CHARS {
        reply.to_owned()
    } else {
        format!("{}…", reply.chars().take(REPLY_LABEL_CHARS - 1).collect::<String>().trim_end())
    }
}

/// Tool uses the cards in an answer show themselves, so they stay out of the steps.
pub fn is_progress_tool(tool: &str) -> bool {
    tool == TODO_TOOL || tool == SUGGEST_TOOL
}

/// (done once, done n times, running) for a tool, e.g. "Ran a command" / "Ran {n} commands" / "Running a command".
fn kind(tool: &str) -> (&'static str, &'static str, &'static str) {
    match tool {
        "Bash" | "PowerShell" => ("Ran a command", "Ran {n} commands", "Running a command"),
        "Read" => ("Read a file", "Read {n} files", "Reading a file"),
        "Edit" | "MultiEdit" | "Write" | "NotebookEdit" => ("Edited a file", "Edited {n} files", "Editing a file"),
        "Glob" | "Grep" => ("Searched the files", "Searched the files {n} times", "Searching the files"),
        "WebSearch" => ("Searched the web", "Searched the web {n} times", "Searching the web"),
        "WebFetch" => ("Opened a page", "Opened {n} pages", "Opening a page"),
        "Task" | "Agent" => ("Ran a subagent", "Ran {n} subagents", "Running a subagent"),
        _ => ("Ran a tool", "Ran {n} tools", "Running a tool"),
    }
}

/// The step in progress, in words: the newest tool use without a result.
pub fn current_step(activity: &[ActivityView]) -> Option<&'static str> {
    activity.iter().filter(|a| a.ok.is_none() && !is_progress_tool(&a.tool)).max_by_key(|a| a.offset).map(|a| kind(&a.tool).2)
}

/// The label of a run of steps with nothing written between them: "Ran a command", "Ran 3 commands",
/// "Running a command" while one has no result yet, "Ran 4 steps" when they differ.
pub fn run_label(run: &[&ActivityView]) -> String {
    let Some(last) = run.last() else { return String::new() };
    if let Some(open) = run.iter().rev().find(|a| a.ok.is_none()) {
        return kind(&open.tool).2.to_owned();
    }
    let same = run.iter().all(|a| kind(&a.tool).0 == kind(&last.tool).0);
    let failed = run.iter().filter(|a| a.ok == Some(false)).count();
    let base = match (same, run.len()) {
        (true, 1) => kind(&last.tool).0.to_owned(),
        (true, n) => kind(&last.tool).1.replace("{n}", &n.to_string()),
        (false, n) => format!("Ran {n} steps"),
    };
    if failed > 0 { format!("{base} · {failed} failed") } else { base }
}

impl AppView {
    /// The server has quick replies (feature `quick_replies`).
    pub fn quick_replies_on(&self) -> bool {
        matches!(&self.connection, crate::backend::Connection::Online { server, .. } if server.quick_replies)
    }

    /// The replies on offer under the open conversation's last answer: it is done, nothing
    /// waits for the user, no draft is typed and the card was not closed.
    pub fn open_quick_replies(&self, cx: &App) -> Option<(aurelius_domain::MessageId, Option<String>, Vec<String>)> {
        if !self.quick_replies_on() || !self.composer.read(cx).value().trim().is_empty() {
            return None;
        }
        let last = self.messages.as_ref()?.messages.iter().rev().find(|m| m.update.is_none() && !self.deleted_queued.contains(&m.id))?;
        if last.role != MessageRole::Assistant || last.status != MessageStatus::Complete || self.dismissed_replies.contains(&last.id) {
            return None;
        }
        if self.pending_approvals().any(|a| Some(a.run_id) == last.run_id) {
            return None;
        }
        let (question, options) = quick_replies(&last.activity)?;
        Some((last.id, question, options))
    }

    /// The card under the last answer, above the composer.
    pub fn quick_replies_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (id, question, options) = self.open_quick_replies(cx)?;
        Some(
            v().id(SharedString::from(format!("quick-replies-{id}"))).aria_label("Suggested replies").gap(px(8.)).px(px(14.)).py(px(11.)).rounded(px(8.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
                .child(h().gap(px(8.)).items_start()
                    .child(div().flex_1().min_w_0().text_size(px(13.)).line_height(px(19.)).text_color(c(theme::TEXT)).children(question))
                    .child(h().id(SharedString::from(format!("quick-replies-close-{id}"))).role(Role::Button).aria_label("Dismiss suggestions").flex_none().w(px(22.)).h(px(22.)).justify_center().items_center().rounded(px(4.)).cursor_pointer()
                        .text_color(c(theme::MUTED)).hover(|s| s.bg(c(theme::SURFACE_2)).text_color(c(theme::TEXT)))
                        .child("✕")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.dismissed_replies.insert(id);
                            cx.notify();
                        }))))
                .child(h().gap(px(8.)).flex_wrap().children(options.into_iter().enumerate().map(|(n, reply)| {
                    let label = reply_label(&reply);
                    let hint = (n == 0).then(|| ui::kbd_always("secondary-enter", cx));
                    ui::button(SharedString::from(format!("quick-reply-{id}-{n}")), label, if n == 0 { ButtonKind::Primary } else { ButtonKind::Outline }, hint)
                        .tooltip(ui::tip(reply.clone(), None))
                        .on_click(cx.listener(move |this, _: &ClickEvent, w, cx| this.send_quick_reply(reply.clone(), w, cx)))
                })))
                .into_any_element(),
        )
    }

    /// Send a suggested reply as the user's message.
    pub fn send_quick_reply(&mut self, reply: String, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |input, cx| input.set_value(reply, window, cx));
        self.send_message(window, cx);
        cx.notify();
    }

    /// Ctrl+Enter: the first reply on offer. Returns whether there was one.
    pub fn send_first_quick_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        match self.open_quick_replies(cx) {
            Some((_, _, options)) => {
                self.send_quick_reply(options[0].clone(), window, cx);
                true
            }
            None => false,
        }
    }

    /// The run's checklist: done, in progress (bold), open.
    pub fn todo_card(&self, message: aurelius_domain::MessageId, list: &[Todo], cx: &mut Context<Self>) -> AnyElement {
        let key = format!("todos-{message}");
        let fold = list.len() > TODO_FOLD_ROWS && !self.expanded_activity.contains(&key);
        let done = list.iter().filter(|t| t.state == TodoState::Done).count();
        let shown: Vec<&Todo> = if fold {
            let at = list.iter().position(|t| t.state == TodoState::Doing).or_else(|| list.iter().position(|t| t.state == TodoState::Open)).unwrap_or(list.len() - 1);
            list.iter().skip(at).take(1).collect()
        } else {
            list.iter().collect()
        };
        v().id(SharedString::from(key.clone())).aria_label("Progress").gap(px(7.)).px(px(14.)).py(px(11.)).rounded(px(10.)).border_1().border_color(c(theme::BORDER)).bg(c(theme::SURFACE))
            .children(shown.into_iter().map(|t| {
                let mark: AnyElement = match t.state {
                    TodoState::Done => ui::check(11., theme::MUTED).into_any_element(),
                    TodoState::Doing => ui::pulse_dot(SharedString::from(format!("todo-{message}")), 7., theme::ACCENT).into_any_element(),
                    TodoState::Open => ui::ring(7., theme::MUTED).into_any_element(),
                };
                h().gap(px(9.)).items_start()
                    .child(div().flex_none().w(px(14.)).h(px(19.)).flex().items_center().justify_center().child(mark))
                    .child(div().flex_1().min_w_0().text_size(px(13.5)).line_height(px(19.))
                        .text_color(c(match t.state { TodoState::Doing => theme::TEXT, TodoState::Done => theme::MUTED, TodoState::Open => theme::TEXT_2 }))
                        .when(t.state == TodoState::Doing, |el| el.font_weight(FontWeight::SEMIBOLD))
                        .child(t.text.clone()))
            }))
            .when(list.len() > TODO_FOLD_ROWS, |el| {
                el.child(h().id(SharedString::from(format!("{key}-toggle"))).role(Role::Button).gap(px(6.)).text_size(px(11.5)).text_color(c(theme::MUTED)).cursor_pointer()
                    .hover(|s| s.text_color(c(theme::TEXT)))
                    .child(if fold { format!("{done} of {} done · show all", list.len()) } else { "Show less".to_string() })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if !this.expanded_activity.remove(&key) {
                            this.expanded_activity.insert(key.clone());
                        }
                        cx.notify();
                    })))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    // Not `use super::*`: gpui_kit exports a `test` macro that would replace #[test].
    use super::{current_step, quick_replies, reply_label, run_label, todos, ActivityView, TodoState, SUGGEST_TOOL, TODO_TOOL};
    use serde_json::json;

    fn act(tool: &str, offset: u32, ok: Option<bool>, input: serde_json::Value) -> ActivityView {
        ActivityView { call_id: format!("{tool}{offset}"), tool: tool.into(), input, output: None, ok, offset, truncated: false, output_bytes: None }
    }

    #[test]
    fn the_latest_todo_list_wins_and_shows_the_active_form() {
        let first = act(TODO_TOOL, 0, Some(true), json!({ "todos": [{ "content": "A", "status": "pending", "activeForm": "Doing A" }] }));
        // From text: a long json! literal exceeds the macro recursion limit.
        let input = r#"{"todos":[{"content":"A","status":"completed","activeForm":"Doing A"},{"content":"B","status":"in_progress","activeForm":"Doing B"},{"content":"C","status":"pending","activeForm":"Doing C"}]}"#;
        let second = act(TODO_TOOL, 5, None, serde_json::from_str(input).unwrap());
        let list = todos(&[first, second]).unwrap();
        assert_eq!(list.iter().map(|t| (t.text.as_str(), t.state)).collect::<Vec<_>>(), vec![("A", TodoState::Done), ("Doing B", TodoState::Doing), ("C", TodoState::Open)]);
        assert!(todos(&[act(TODO_TOOL, 0, None, json!({ "todos": [] }))]).is_none());
        assert!(todos(&[act("Bash", 0, None, json!({}))]).is_none());
    }

    #[test]
    fn quick_replies_need_two_options_and_keep_three() {
        let one = act(SUGGEST_TOOL, 3, Some(true), json!({ "question": "Clean up?", "options": ["Yes"] }));
        assert!(quick_replies(&[one]).is_none());
        let input = r#"{"question":" Clean up? ","options":["Yes, clean up"," ","Keep them","No","Later"]}"#;
        let four = act(SUGGEST_TOOL, 3, Some(true), serde_json::from_str(input).unwrap());
        let (q, options) = quick_replies(&[four]).unwrap();
        assert_eq!(q.as_deref(), Some("Clean up?"));
        assert_eq!(options, vec!["Yes, clean up", "Keep them", "No"]);
        assert_eq!(reply_label(&"x".repeat(60)).chars().count(), 40);
        assert_eq!(reply_label("short"), "short");
    }

    #[test]
    fn steps_are_named_like_claude_desktop() {
        let done = |tool: &str, n| act(tool, n, Some(true), json!({}));
        let running = act("Bash", 9, None, json!({}));
        let (a, b) = (done("Bash", 1), done("Bash", 2));
        assert_eq!(run_label(&[&a]), "Ran a command");
        assert_eq!(run_label(&[&a, &b]), "Ran 2 commands");
        assert_eq!(run_label(&[&a, &running]), "Running a command");
        let read = done("Read", 3);
        assert_eq!(run_label(&[&a, &read]), "Ran 2 steps");
        assert_eq!(current_step(&[a.clone(), running]), Some("Running a command"));
        assert_eq!(current_step(&[a, act(TODO_TOOL, 4, None, json!({}))]), None);
    }
}
