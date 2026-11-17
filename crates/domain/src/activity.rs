//! Tool use as clients first see it: short. A conversation with hundreds of
//! tool calls (files written whole in the input, long command output) would
//! otherwise be megabytes on every load and every live update. The whole call
//! stays stored and comes back with `activity.get`.

use crate::views::ActivityView;

/// Most of an output a preview keeps, in bytes.
pub const OUTPUT_PREVIEW_BYTES: usize = 2 * 1024;
/// Most of any one string in the input a preview keeps, in bytes.
pub const INPUT_STRING_PREVIEW_BYTES: usize = 1024;

/// The view with its output and long input strings cut at a character
/// boundary; `truncated` and `output_bytes` say what was cut.
pub fn preview(mut view: ActivityView) -> ActivityView {
    if let Some(output) = view.output.as_mut() {
        if output.len() > OUTPUT_PREVIEW_BYTES {
            view.output_bytes = Some(u32::try_from(output.len()).unwrap_or(u32::MAX));
            cut(output, OUTPUT_PREVIEW_BYTES);
            view.truncated = true;
        }
    }
    if shorten(&mut view.input) {
        view.truncated = true;
    }
    view
}

/// Shortens every string in `value` longer than the input cap; true when any was.
fn shorten(value: &mut serde_json::Value) -> bool {
    match value {
        serde_json::Value::String(text) if text.len() > INPUT_STRING_PREVIEW_BYTES => {
            cut(text, INPUT_STRING_PREVIEW_BYTES);
            true
        }
        serde_json::Value::Array(items) => items.iter_mut().fold(false, |any, item| shorten(item) | any),
        serde_json::Value::Object(fields) => fields.values_mut().fold(false, |any, field| shorten(field) | any),
        _ => false,
    }
}

fn cut(text: &mut String, max: usize) {
    let mut end = max.min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    text.truncate(end);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(input: serde_json::Value, output: Option<&str>) -> ActivityView {
        ActivityView { call_id: "1".into(), tool: "Write".into(), input, output: output.map(str::to_owned), ok: Some(true), offset: 0, truncated: false, output_bytes: None, parent_call_id: None }
    }

    #[test]
    fn short_calls_stay_as_they_are() {
        let view = call(serde_json::json!({ "command": "ls" }), Some("a\nb"));
        assert_eq!(preview(view.clone()), view);
        let running = call(serde_json::json!({ "command": "ls" }), None);
        assert_eq!(preview(running.clone()), running);
    }

    #[test]
    fn long_output_and_input_strings_are_cut_with_their_size() {
        let content = "é".repeat(2_000); // 4,000 bytes, two per character
        let output = "x".repeat(OUTPUT_PREVIEW_BYTES + 10);
        let view = preview(call(serde_json::json!({ "file_path": "a.md", "content": content, "edits": [{ "new_string": content }] }), Some(&output)));
        assert!(view.truncated);
        assert_eq!(view.output_bytes, Some((OUTPUT_PREVIEW_BYTES + 10) as u32));
        assert_eq!(view.output.as_deref().map(str::len), Some(OUTPUT_PREVIEW_BYTES));
        assert_eq!(view.input["file_path"], "a.md");
        assert_eq!(view.input["content"].as_str().unwrap().len(), INPUT_STRING_PREVIEW_BYTES);
        assert_eq!(view.input["edits"][0]["new_string"].as_str().unwrap().len(), INPUT_STRING_PREVIEW_BYTES);
    }

    #[test]
    fn a_cut_never_splits_a_character() {
        let mut text = format!("{}é", "a".repeat(INPUT_STRING_PREVIEW_BYTES - 1));
        text.push_str("tail");
        let view = preview(call(serde_json::json!({ "content": text }), None));
        assert_eq!(view.input["content"].as_str().unwrap().len(), INPUT_STRING_PREVIEW_BYTES - 1);
        assert_eq!(view.output_bytes, None);
    }
}
