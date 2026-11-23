//! A design while it is written: the model streams a `Write` call's input as
//! JSON in pieces (`input_json_delta`), long before the call is complete. For
//! an HTML file the app shows what is there so far, as Claude Desktop does with
//! an artifact. This reads the file path and the content written so far out of
//! that unfinished JSON.

/// The tool whose input is a whole file.
pub const DRAFT_TOOL: &str = "Write";
/// A draft is reported again only when its input grew by this many bytes, so a
/// long file is not decoded on every few characters. A model writes a few hundred
/// bytes a second; the dispatcher sends at most one draft per `DRAFT_EVERY`.
pub const DRAFT_STEP_BYTES: usize = 256;
/// Until the path is known (or when the content came first), the input is read
/// again after this many bytes, so a long input is not decoded on every piece.
pub const DRAFT_PATH_STEP_BYTES: usize = 64;

/// Whether `path` is a file the app shows as a design (HTML).
pub fn is_design(path: &str) -> bool {
    let lower = path.to_ascii_lowercase();
    lower.ends_with(".html") || lower.ends_with(".htm")
}

/// The file path and the content written so far, from the unfinished input
/// JSON of a `Write` call. `None` until the path is complete; the content may
/// be cut anywhere, also inside an escape, and is then given up to the last
/// whole character. Only the object's own keys count.
pub fn partial_write(json: &str) -> Option<(String, String)> {
    let mut rest = json.trim_start().strip_prefix('{')?;
    let (mut path, mut content) = (None, None);
    loop {
        rest = rest.trim_start().trim_start_matches(',').trim_start();
        let Some(after) = rest.strip_prefix('"') else {
            break;
        };
        let (key, whole, used) = string_prefix(after);
        if !whole {
            break;
        }
        rest = after[used..].trim_start();
        let Some(after) = rest.strip_prefix(':') else {
            break;
        };
        rest = after.trim_start();
        let Some(after) = rest.strip_prefix('"') else {
            // Not a string (Write has none, but be safe): skip to the next key we can read.
            match skip_value(rest) {
                Some(next) => {
                    rest = next;
                    continue;
                }
                None => break,
            }
        };
        let (value, whole, used) = string_prefix(after);
        match key.as_str() {
            "file_path" if whole => path = Some(value),
            "content" => content = Some(value),
            _ => {}
        }
        if !whole {
            break;
        }
        rest = &after[used..];
    }
    Some((path?, content.unwrap_or_default()))
}

/// The rest after a non-string value, when it ends before `json` does.
fn skip_value(json: &str) -> Option<&str> {
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    for (i, c) in json.char_indices() {
        if in_string {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => in_string = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' | '[' => depth += 1,
            '}' | ']' if depth > 0 => depth -= 1,
            ',' | '}' if depth == 0 => return Some(&json[i..]),
            _ => {}
        }
    }
    None
}

/// Decodes a JSON string body (after its opening quote) as far as it goes:
/// the text, whether the closing quote came, and the bytes used up to and
/// including it. An escape cut short ends the text before it.
fn string_prefix(body: &str) -> (String, bool, usize) {
    let mut out = String::with_capacity(body.len());
    let bytes = body.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => return (out, true, i + 1),
            b'\\' => {
                let Some(&kind) = bytes.get(i + 1) else { break };
                let simple = match kind {
                    b'"' => Some('"'),
                    b'\\' => Some('\\'),
                    b'/' => Some('/'),
                    b'b' => Some('\u{8}'),
                    b'f' => Some('\u{c}'),
                    b'n' => Some('\n'),
                    b'r' => Some('\r'),
                    b't' => Some('\t'),
                    _ => None,
                };
                if let Some(c) = simple {
                    out.push(c);
                    i += 2;
                    continue;
                }
                if kind != b'u' {
                    // Not valid JSON: stop rather than guess.
                    break;
                }
                let Some(unit) = hex4(bytes, i + 2) else {
                    break;
                };
                if (0xD800..0xDC00).contains(&unit) {
                    // A surrogate pair: both halves must be there.
                    let low = (bytes.get(i + 6) == Some(&b'\\') && bytes.get(i + 7) == Some(&b'u'))
                        .then(|| hex4(bytes, i + 8))
                        .flatten();
                    let Some(low) = low else { break };
                    let c = 0x10000
                        + ((u32::from(unit) - 0xD800) << 10)
                        + (u32::from(low).wrapping_sub(0xDC00) & 0x3FF);
                    out.push(char::from_u32(c).unwrap_or('\u{FFFD}'));
                    i += 12;
                } else {
                    out.push(char::from_u32(u32::from(unit)).unwrap_or('\u{FFFD}'));
                    i += 6;
                }
            }
            _ => {
                // A plain run up to the next quote or escape; the input is UTF-8, so is the run.
                let end = bytes[i..]
                    .iter()
                    .position(|b| *b == b'"' || *b == b'\\')
                    .map_or(bytes.len(), |n| i + n);
                out.push_str(&body[i..end]);
                i = end;
            }
        }
    }
    (out, false, bytes.len())
}

fn hex4(bytes: &[u8], at: usize) -> Option<u16> {
    let digits = std::str::from_utf8(bytes.get(at..at + 4)?).ok()?;
    u16::from_str_radix(digits, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_content_so_far_comes_out_of_unfinished_json() {
        let json =
            r#"{"file_path": "/p/design.html", "content": "<html>\n<body class=\"a\">Caf\u00e9"#;
        assert_eq!(
            partial_write(json),
            Some((
                "/p/design.html".into(),
                "<html>\n<body class=\"a\">Café".into()
            ))
        );
    }

    #[test]
    fn a_whole_call_gives_its_whole_content() {
        let input =
            serde_json::json!({ "file_path": "/p/a.html", "content": "<p>\"x\" \\ é 😀</p>\n" });
        let json = input.to_string();
        assert_eq!(
            partial_write(&json),
            Some(("/p/a.html".into(), "<p>\"x\" \\ é 😀</p>\n".into()))
        );
        // Escaped the way a model may write it, with \u escapes and a surrogate pair.
        assert_eq!(
            partial_write(r#"{"file_path":"a.html","content":"\u00e9\ud83d\ude00"}"#)
                .unwrap()
                .1,
            "é😀"
        );
    }

    #[test]
    fn a_cut_escape_ends_the_content_before_it() {
        for cut in [
            r#"{"file_path":"a.html","content":"ab\"#,
            r#"{"file_path":"a.html","content":"ab\u00"#,
            r#"{"file_path":"a.html","content":"ab\ud83d"#,
            r#"{"file_path":"a.html","content":"ab\ud83d\u"#,
        ] {
            assert_eq!(partial_write(cut).unwrap().1, "ab", "{cut}");
        }
    }

    #[test]
    fn there_is_no_draft_until_the_path_is_whole() {
        assert_eq!(partial_write(""), None);
        assert_eq!(partial_write("{"), None);
        assert_eq!(partial_write(r#"{"file_path":"/p/des"#), None);
        assert_eq!(
            partial_write(r#"{"file_path":"/p/design.html""#),
            Some(("/p/design.html".into(), String::new()))
        );
        // The content before the path: known once the path is there.
        assert_eq!(
            partial_write(r#"{"content":"<b>x</b>","file_path":"/p/a.html"}"#),
            Some(("/p/a.html".into(), "<b>x</b>".into()))
        );
    }

    #[test]
    fn only_html_files_are_designs() {
        assert!(is_design("/p/Design.HTML") && is_design("C:\\p\\a.htm"));
        assert!(!is_design("/p/a.md") && !is_design("/p/html") && !is_design("/p/a.html.bak"));
    }

    #[test]
    fn a_key_with_a_value_that_is_not_a_string_is_skipped() {
        assert_eq!(
            partial_write(r#"{"x": {"y": [1, "}"]}, "file_path": "a.html", "content": "z"#),
            Some(("a.html".into(), "z".into()))
        );
    }
}
