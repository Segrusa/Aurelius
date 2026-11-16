//! Tool calls written out as text. A model that is told about tools its run does
//! not have (no bridge, no folder) sometimes writes the calls as `<invoke name="…">`
//! lines instead of making them. Such an answer did nothing of what it shows, so it
//! must never be posted as if it were one (seen on Windows, 2026-11-16).

/// The explanation that replaces such an answer in the conversation.
pub const WRITTEN_CALLS_EXPLANATION: &str = "This answer was stopped: it wrote tool calls out as text instead of making them, so nothing it shows was done. The run probably did not have those tools; try again, and tell the team if it happens again.";

/// How such a line starts, after its `<`.
const CALL_TAGS: [&str; 3] = ["invoke name=", "function_calls>", "parameter name="];

/// Whether `text` holds tool calls written out as text: a line that starts with
/// `<invoke name=`, `<function_calls>` or `<parameter name=`, outside a fenced code
/// block. Quoting them in a code block (or inline code) is not one.
pub fn has_written_calls(text: &str) -> bool {
    let mut fenced = false;
    for line in text.lines() {
        let line = line.trim_start();
        if line.starts_with("```") || line.starts_with("~~~") {
            fenced = !fenced;
            continue;
        }
        if fenced {
            continue;
        }
        if let Some(tag) = line.strip_prefix('<') {
            if CALL_TAGS.iter().any(|t| tag.starts_with(t)) {
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calls_written_as_text_are_found() {
        let text = "Ik begin met het bord.\n\n<invoke name=\"list_tickets\"> </invoke>\n<invoke name=\"read_group_chat\"> </invoke>";
        assert!(has_written_calls(text));
        assert!(has_written_calls("<function_calls>\n<invoke name=\"Bash\">\n<parameter name=\"command\">ls</parameter>\n</invoke>"));
        assert!(has_written_calls("   <parameter name=\"message\">@Olivier hi</parameter>"));
    }

    #[test]
    fn calls_quoted_in_code_are_not() {
        assert!(!has_written_calls("The bug looks like this:\n```\n<invoke name=\"list_tickets\"> </invoke>\n```\nso the bridge was missing."));
        assert!(!has_written_calls("It wrote `<invoke name=\"Bash\">` as text."));
        assert!(!has_written_calls("A plain answer about <b>bold</b> HTML and an invoice."));
    }
}
