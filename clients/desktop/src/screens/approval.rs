//! What an approval asks, readable: the command, the file and its change, the
//! URL. Shown inline in the chat, on the ASKED card and in the inbox.

use crate::app::AppView;
use crate::theme::{self, c};
use crate::ui::{self, h, v};
use aurelius_protocol::ApprovalView;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

/// Lines shown before "Show all".
const FOLDED_LINES: usize = 10;
/// Lines shown after "Show all" (beyond this the inbox details have it all).
const FULL_LINES: usize = 400;
/// A single line is cut here.
const LINE_CHARS: usize = 300;

/// The core of what an approval would do.
#[derive(Debug, Clone, PartialEq)]
pub enum Detail {
    Command { command: String, description: Option<String> },
    /// A new file's path and content.
    Write { path: String, content: String },
    /// A file's path and its replacements (old → new).
    Edit { path: String, edits: Vec<(String, String)> },
    Path(String),
    Url { url: String, prompt: Option<String> },
    Query(String),
    /// Anything else: its arguments as key: value lines.
    Fields(Vec<(String, String)>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    /// Short, for lists: "Run a command", "Edit a file".
    pub title: String,
    /// One line: the command, the path, the URL.
    pub gist: String,
    pub detail: Detail,
}

/// What `a` asks, with paths relative to the first of `folders` they are in.
pub fn approval_summary(a: &ApprovalView, folders: &[String]) -> Summary {
    let args = &a.arguments;
    let text = |k: &str| args[k].as_str().map(str::to_owned);
    let path = |k: &str| relative(&text(k).unwrap_or_default(), folders);
    let tool = tool_name(&a.tool);
    let (title, detail) = match tool {
        "Bash" | "PowerShell" => (
            "Run a command",
            Detail::Command { command: text("command").unwrap_or_default(), description: text("description").filter(|d| !d.trim().is_empty()) },
        ),
        "Write" => ("Write a file", Detail::Write { path: path("file_path"), content: text("content").unwrap_or_default() }),
        "Edit" => ("Edit a file", Detail::Edit { path: path("file_path"), edits: vec![(text("old_string").unwrap_or_default(), text("new_string").unwrap_or_default())] }),
        "MultiEdit" => {
            let edits = args["edits"].as_array().map(|e| {
                e.iter().map(|e| (e["old_string"].as_str().unwrap_or("").to_owned(), e["new_string"].as_str().unwrap_or("").to_owned())).collect()
            });
            ("Edit a file", Detail::Edit { path: path("file_path"), edits: edits.unwrap_or_default() })
        }
        "NotebookEdit" => ("Edit a notebook", Detail::Path(path("notebook_path"))),
        "WebFetch" => ("Open a web page", Detail::Url { url: text("url").unwrap_or_default(), prompt: text("prompt").filter(|p| !p.trim().is_empty()) }),
        "WebSearch" => ("Search the web", Detail::Query(text("query").unwrap_or_default())),
        "Artifact" | "ArtifactData" | "ArtifactComments" => (if publishes(tool, args) { "Publish an artifact" } else { "Change an artifact" }, Detail::Fields(fields(args))),
        "mcp__aurelius__screenshot" => ("Screenshot", Detail::Fields(fields(args))),
        "mcp__aurelius__list_windows" => ("List open windows", Detail::Fields(fields(args))),
        other => (other, Detail::Fields(fields(args))),
    };
    let gist = match &detail {
        Detail::Command { command, .. } => first_line(command),
        Detail::Write { path, .. } | Detail::Edit { path, .. } | Detail::Path(path) => path.clone(),
        Detail::Url { url, .. } => url.clone(),
        Detail::Query(q) => q.clone(),
        Detail::Fields(f) => f.iter().map(|(k, v)| format!("{k}: {}", first_line(v))).collect::<Vec<_>>().join(" · "),
    };
    Summary { title: title.to_owned(), gist, detail }
}

/// An Artifact call without an action (or `publish`) publishes a page; other actions change one.
fn publishes(tool: &str, args: &serde_json::Value) -> bool {
    tool == "Artifact" && matches!(args["action"].as_str(), None | Some("publish"))
}

/// The tool without Claude Code's `claude.` prefix.
pub fn tool_name(tool: &str) -> &str {
    tool.strip_prefix("claude.").unwrap_or(tool)
}

/// "Alex wants to run a command", for the inbox headline.
pub fn sentence(a: &ApprovalView) -> String {
    let who = &a.requester;
    match tool_name(&a.tool) {
        "Write" => format!("{who} wants to write a file"),
        "Edit" | "MultiEdit" | "NotebookEdit" => format!("{who} wants to edit a file"),
        "Bash" | "PowerShell" => format!("{who} wants to run a command"),
        "WebFetch" => format!("{who} wants to open a web page"),
        "WebSearch" => format!("{who} wants to search the web"),
        "Artifact" | "ArtifactData" | "ArtifactComments" if publishes(tool_name(&a.tool), &a.arguments) => format!("{who} wants to publish an artifact to claude.ai"),
        "Artifact" | "ArtifactData" | "ArtifactComments" => format!("{who} wants to change an artifact on claude.ai"),
        "mcp__aurelius__screenshot" => format!("{who} wants to see your screen"),
        "mcp__aurelius__list_windows" => format!("{who} wants to see which windows are open"),
        other => format!("{who} wants to use {other}"),
    }
}

/// `path` relative to the first folder it lies in; else as it is.
pub fn relative(path: &str, folders: &[String]) -> String {
    let norm = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_ascii_lowercase();
    let p = norm(path);
    for folder in folders.iter().filter(|f| !f.trim().is_empty()) {
        let f = norm(folder);
        if p == f {
            return ".".into();
        }
        if p.starts_with(&f) && p.as_bytes().get(f.len()) == Some(&b'/') {
            // ASCII lowercasing and slash replacing keep byte lengths: cut the original.
            if let Some(rest) = path.get(f.len() + 1..) {
                return rest.to_owned();
            }
        }
    }
    path.to_owned()
}

/// A plain chat works in its own folder `…/workspaces/<conversation id>`: that
/// folder, when the approval's path lies in it.
fn own_workspace(a: &ApprovalView, conversation: &str) -> Option<String> {
    let path = ["file_path", "notebook_path"].iter().find_map(|k| a.arguments[*k].as_str())?;
    let norm = path.replace('\\', "/").to_ascii_lowercase();
    let at = norm.find(&format!("/workspaces/{}", conversation.to_ascii_lowercase()))?;
    Some(path[..at + "/workspaces/".len() + conversation.len()].to_owned())
}

/// Arguments as key: value lines, values as plain text (strings unquoted).
fn fields(args: &serde_json::Value) -> Vec<(String, String)> {
    match args {
        serde_json::Value::Object(map) => map
            .iter()
            .filter(|(_, v)| !v.is_null())
            .map(|(k, v)| (k.clone(), match v {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            }))
            .collect(),
        serde_json::Value::Null => Vec::new(),
        other => vec![("value".into(), other.to_string())],
    }
}

fn first_line(text: &str) -> String {
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let first = lines.next().unwrap_or("").trim().to_owned();
    if lines.next().is_some() { format!("{first} …") } else { first }
}

/// At most `max` lines of `text`, each cut at `LINE_CHARS`; and how many lines were left out.
pub fn clip(text: &str, max: usize) -> (Vec<String>, usize) {
    let lines: Vec<&str> = text.lines().collect();
    let shown = lines
        .iter()
        .take(max)
        .map(|l| if l.chars().count() > LINE_CHARS { format!("{}…", l.chars().take(LINE_CHARS).collect::<String>()) } else { (*l).to_owned() })
        .collect();
    (shown, lines.len().saturating_sub(max))
}

/// The lines of a summary's body: (sign, text) with sign ' ', '+' or '-'.
fn body_lines(detail: &Detail) -> Vec<(char, String)> {
    let plain = |t: &str| t.lines().map(|l| (' ', l.to_owned())).collect::<Vec<_>>();
    match detail {
        Detail::Command { command, .. } => plain(command),
        Detail::Write { content, .. } => content.lines().map(|l| ('+', l.to_owned())).collect(),
        Detail::Edit { edits, .. } => {
            let mut out = Vec::new();
            for (i, (old, new)) in edits.iter().enumerate() {
                if i > 0 {
                    out.push((' ', "⋯".to_owned()));
                }
                out.extend(old.lines().map(|l| ('-', l.to_owned())));
                out.extend(new.lines().map(|l| ('+', l.to_owned())));
            }
            out
        }
        Detail::Fields(f) => f.iter().flat_map(|(k, v)| {
            let mut lines = v.lines();
            let first = (' ', format!("{k}: {}", lines.next().unwrap_or("")));
            std::iter::once(first).chain(lines.map(|l| (' ', format!("  {l}")))).collect::<Vec<_>>()
        }).collect(),
        Detail::Path(_) | Detail::Url { .. } | Detail::Query(_) => Vec::new(),
    }
}

impl AppView {
    /// Folders an approval's paths are shown relative to: the manager's own folder, the project folder.
    pub fn approval_folders(&self, a: &ApprovalView) -> Vec<String> {
        let mut out = Vec::new();
        let task = a.task_id.and_then(|id| self.tasks.iter().find(|t| t.id == id))
            .or_else(|| a.conversation_id.and_then(|cid| self.tasks.iter().find(|t| t.conversation_id == Some(cid))));
        let manager = task.and_then(|t| t.employee_id).and_then(|id| self.managers.iter().find(|m| m.id == id)).or_else(|| {
            a.conversation_id.and_then(|cid| self.managers.iter().find(|m| m.conversation_id == cid || m.conversations.iter().any(|s| s.id == cid)))
        });
        // Its own folder, and in a task the task's project folder.
        if let Some(m) = manager {
            out.extend(m.folder.clone());
        }
        if let Some(t) = task {
            out.extend(self.projects.iter().find(|p| p.id == t.project_id).and_then(|p| p.folder.clone()));
        }
        out.extend(a.conversation_id.and_then(|cid| own_workspace(a, &cid.to_string())));
        out
    }

    /// The readable summary of `a`: the gist on top, then the command, content or
    /// change, folded to a few lines with "Show all". `key` keeps element ids apart
    /// when the same approval shows twice on a screen.
    pub fn approval_detail(&self, a: &ApprovalView, key: &str, cx: &mut Context<Self>) -> AnyElement {
        let s = approval_summary(a, &self.approval_folders(a));
        let mono = ui::mono(cx);
        let full = self.approval_full.contains(&a.id);
        let lines = body_lines(&s.detail);
        let max = if full { FULL_LINES } else { FOLDED_LINES };
        let hidden = lines.len().saturating_sub(max);
        let shown: Vec<(char, String)> = lines.into_iter().take(max).map(|(sign, l)| {
            let (mut cut, _) = clip(&l, 1);
            (sign, cut.pop().unwrap_or_default())
        }).collect();
        let head: Option<AnyElement> = match &s.detail {
            Detail::Command { description, .. } => description.clone().map(|d| div().text_size(px(12.5)).text_color(c(theme::TEXT_2)).child(d).into_any_element()),
            Detail::Write { path, .. } | Detail::Edit { path, .. } | Detail::Path(path) => {
                Some(div().font_family(mono.clone()).text_size(px(12.5)).text_color(c(theme::TEXT)).child(path.clone()).into_any_element())
            }
            Detail::Url { url, prompt } => Some(v().gap(px(3.))
                .child(div().font_family(mono.clone()).text_size(px(12.5)).text_color(c(theme::ACCENT_TEXT)).child(url.clone()))
                .when_some(prompt.clone(), |el, p| el.child(div().text_size(px(12.)).text_color(c(theme::TEXT_2)).child(clip(&p, 3).0.join(" "))))
                .into_any_element()),
            Detail::Query(q) => Some(div().text_size(px(13.)).text_color(c(theme::TEXT)).child(format!("“{q}”")).into_any_element()),
            Detail::Fields(_) => None,
        };
        let id = a.id;
        let copy_text = match &s.detail {
            Detail::Command { command, .. } => command.clone(),
            Detail::Write { content, .. } => content.clone(),
            Detail::Edit { edits, .. } => edits.iter().map(|(_, new)| new.as_str()).collect::<Vec<_>>().join("

"),
            Detail::Path(p) => p.clone(),
            Detail::Url { url, .. } => url.clone(),
            Detail::Query(q) => q.clone(),
            Detail::Fields(f) => f.iter().map(|(k, v)| format!("{k}: {v}")).collect::<Vec<_>>().join("
"),
        };
        let copy = self.copy_button(format!("approval-{key}-{id}"), copy_text, cx);
        v().gap(px(6.)).w_full().min_w_0()
            .child(h().gap(px(8.)).items_start().child(div().flex_1().min_w_0().children(head)).child(copy))
            .when(!shown.is_empty(), |el| {
                el.child(v().w_full().min_w_0().py(px(6.)).rounded(px(6.)).bg(c(theme::SURFACE)).border_1().border_color(c(theme::BORDER))
                    .font_family(mono.clone()).text_size(px(12.)).line_height(px(18.))
                    .children(shown.into_iter().map(|(sign, line)| {
                        let (color, bg) = match sign {
                            '+' => (theme::ACCENT_TEXT, Some(Hsla { a: 0.12, ..c(theme::ACCENT) })),
                            '-' => (theme::NEGATIVE, Some(Hsla { a: 0.12, ..c(theme::NEGATIVE) })),
                            _ => (theme::TEXT, None),
                        };
                        h().items_start().w_full().min_w_0().px(px(10.)).when_some(bg, |el, bg| el.bg(bg))
                            .when(sign != ' ', |el| el.child(div().w(px(14.)).flex_none().text_color(c(color)).child(if sign == '-' { "−" } else { "+" })))
                            .child(div().flex_1().min_w_0().text_color(c(if sign == ' ' { theme::TEXT } else { theme::TEXT_2 })).child(if line.is_empty() { " ".to_owned() } else { line }))
                    })))
            })
            .when(hidden > 0 || full, |el| {
                el.child(div().id(SharedString::from(format!("approval-all-{key}-{id}"))).text_size(px(11.5)).cursor_pointer()
                    .text_color(c(theme::ACCENT_TEXT)).hover(|s| s.text_color(c(theme::TEXT)))
                    .child(if full {
                        if hidden > 0 { format!("Show less · {hidden} more lines not shown") } else { "Show less".to_owned() }
                    } else {
                        format!("Show all · {hidden} more line{}", if hidden == 1 { "" } else { "s" })
                    })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if !this.approval_full.remove(&id) {
                            this.approval_full.insert(id);
                        }
                        cx.notify();
                    })))
            })
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{Detail, Summary, approval_summary, body_lines, clip, own_workspace, relative, sentence, FOLDED_LINES, LINE_CHARS};
    use aurelius_protocol::ApprovalView;
    use serde_json::json;

    fn approval(tool: &str, arguments: serde_json::Value) -> ApprovalView {
        ApprovalView {
            id: aurelius_domain::ApprovalId::new(),
            status: aurelius_domain::approval::ApprovalStatus::Pending,
            tool: tool.into(),
            effect: "local_write".into(),
            arguments,
            resources: vec![],
            account: "me".into(),
            reasons: vec![],
            action_digest: "d".into(),
            requested_at_ms: 0,
            expires_at_ms: 0,
            run_id: aurelius_domain::RunId::new(),
            conversation_id: None,
            requester: "Alex".into(),
            task_id: None,
            always_allow_eligible: false,
        }
    }

    const ROOT: &str = r"C:\Work\atlas";

    fn summary(tool: &str, args: serde_json::Value) -> Summary {
        approval_summary(&approval(tool, args), &[ROOT.to_owned()])
    }

    #[test]
    fn a_command_shows_the_command_and_its_description() {
        let s = summary("claude.Bash", json!({"command": "cargo test -p x\ncargo build", "description": "Run the tests"}));
        assert_eq!(s.title, "Run a command");
        assert_eq!(s.gist, "cargo test -p x …");
        assert_eq!(s.detail, Detail::Command { command: "cargo test -p x\ncargo build".into(), description: Some("Run the tests".into()) });
        let p = summary("claude.PowerShell", json!({"command": "Get-ChildItem"}));
        assert_eq!(p.detail, Detail::Command { command: "Get-ChildItem".into(), description: None });
        assert_eq!(body_lines(&p.detail), vec![(' ', "Get-ChildItem".to_owned())]);
    }

    #[test]
    fn an_artifact_publish_reads_as_one() {
        let args = json!({"file_path": r"C:\Temp\page.html", "icon": "page", "description": "Q3 revenue"});
        assert_eq!(summary("claude.Artifact", args.clone()).title, "Publish an artifact");
        assert_eq!(sentence(&approval("claude.Artifact", args)), "Alex wants to publish an artifact to claude.ai");
        assert_eq!(summary("claude.Artifact", json!({"action": "delete", "url": "u"})).title, "Change an artifact");
        assert_eq!(sentence(&approval("claude.ArtifactComments", json!({"action": "reply"}))), "Alex wants to change an artifact on claude.ai");
    }

    #[test]
    fn a_write_shows_the_relative_path_and_its_content() {
        let s = summary("claude.Write", json!({"file_path": r"C:\Work\atlas\src\main.rs", "content": "fn main() {}\n"}));
        assert_eq!(s.title, "Write a file");
        assert_eq!(s.gist, r"src\main.rs");
        assert_eq!(body_lines(&s.detail), vec![('+', "fn main() {}".to_owned())]);
    }

    #[test]
    fn an_edit_shows_old_and_new_lines() {
        let s = summary("claude.Edit", json!({"file_path": "C:/work/Atlas/a.rs", "old_string": "let a = 1;", "new_string": "let a = 2;\nlet b = 3;"}));
        assert_eq!(s.title, "Edit a file");
        assert_eq!(s.gist, "a.rs", "case and slashes don't matter on Windows paths");
        assert_eq!(body_lines(&s.detail), vec![('-', "let a = 1;".into()), ('+', "let a = 2;".into()), ('+', "let b = 3;".into())]);
        let m = summary("claude.MultiEdit", json!({"file_path": "/elsewhere/b.rs", "edits": [{"old_string": "x", "new_string": "y"}, {"old_string": "p", "new_string": "q"}]}));
        assert_eq!(m.gist, "/elsewhere/b.rs", "outside the folder: as it is");
        assert_eq!(body_lines(&m.detail), vec![('-', "x".into()), ('+', "y".into()), (' ', "⋯".into()), ('-', "p".into()), ('+', "q".into())]);
    }

    #[test]
    fn a_plain_chat_shows_paths_in_its_own_folder_relative() {
        let mut a = approval("claude.Edit", json!({"file_path": r"C:\Data\workspaces\CONV-1\src\main.rs"}));
        a.conversation_id = None;
        let folder = own_workspace(&a, "conv-1").expect("in its own folder");
        assert_eq!(folder, r"C:\Data\workspaces\CONV-1");
        assert_eq!(approval_summary(&a, &[folder]).gist, r"src\main.rs");
        assert_eq!(own_workspace(&a, "conv-2"), None);
    }

    #[test]
    fn a_notebook_edit_shows_its_path() {
        let s = summary("claude.NotebookEdit", json!({"notebook_path": r"C:\Work\atlas\n.ipynb", "new_source": "x"}));
        assert_eq!(s.detail, Detail::Path("n.ipynb".into()));
        assert!(body_lines(&s.detail).is_empty());
    }

    #[test]
    fn web_tools_show_the_url_or_the_query() {
        let f = summary("claude.WebFetch", json!({"url": "https://example.com/a", "prompt": "Summarise"}));
        assert_eq!((f.title.as_str(), f.gist.as_str()), ("Open a web page", "https://example.com/a"));
        let s = summary("claude.WebSearch", json!({"query": "gpui scroll"}));
        assert_eq!((s.title.as_str(), s.gist.as_str()), ("Search the web", "gpui scroll"));
    }

    #[test]
    fn aurelius_and_unknown_tools_show_key_value_lines() {
        let s = summary("claude.mcp__aurelius__screenshot", json!({"window": "Chrome"}));
        assert_eq!((s.title.as_str(), s.gist.as_str()), ("Screenshot", "window: Chrome"));
        let u = summary("claude.mcp__github__create_issue", json!({"body": "one\ntwo", "labels": ["a"], "title": "Bug"}));
        assert_eq!(u.title, "mcp__github__create_issue");
        let Detail::Fields(mut f) = u.detail.clone() else { panic!("fields expected") };
        f.sort();
        assert_eq!(f, vec![("body".into(), "one\ntwo".into()), ("labels".into(), r#"["a"]"#.into()), ("title".into(), "Bug".to_owned())]);
        let lines = body_lines(&u.detail);
        assert!(lines.windows(2).any(|w| w == [(' ', "body: one".to_owned()), (' ', "  two".to_owned())]), "{lines:?}");
        assert_eq!(sentence(&approval("claude.Bash", json!({}))), "Alex wants to run a command");
    }

    #[test]
    fn long_content_is_cut() {
        let text = (0..30).map(|i| i.to_string()).collect::<Vec<_>>().join("\n");
        let (shown, hidden) = clip(&text, FOLDED_LINES);
        assert_eq!((shown.len(), hidden), (10, 20));
        let (line, _) = clip(&"x".repeat(1000), 1);
        assert_eq!(line[0].chars().count(), LINE_CHARS + 1);
        assert_eq!(relative(r"C:\Work\atlas", &[ROOT.into()]), ".");
        assert_eq!(relative(r"C:\Work\atlas2\x", &[ROOT.into()]), r"C:\Work\atlas2\x");
    }
}
